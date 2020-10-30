use crate::{matrix, strapi};
use std::{collections::HashMap, convert::TryFrom, time::Duration};

use futures_util::stream::TryStreamExt as _;
use ruma::{
    api::client::r0::{
        membership::join_room_by_id,
        sync::sync_events::{self, InvitedRoom},
    },
    events::{
        room::{
            member::MembershipState,
            message::{MessageEventContent, TextMessageEventContent},
        },
        AnySyncMessageEvent, AnySyncRoomEvent, AnySyncStateEvent, SyncMessageEvent, SyncStateEvent,
    },
    presence::PresenceState,
    RoomId, UserId,
};
use ruma_client::{self, HttpsClient};
use serde::Serialize;

mod backend;
mod messages;

#[derive(Serialize)]
struct Message {
    room: String,
    user: String,
    message: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct RoomInfo {
    id: String,
    name: Option<String>,
    alias: Option<String>,
}

#[derive(Serialize)]
struct StrapiEvent<'a> {
    room: RoomInfo,
    data: &'a SyncMessageEvent<MessageEventContent>,
}

struct State {
    client: HttpsClient,
    bot_id: UserId,
    admin_users: Vec<String>,
    all_room_info: HashMap<RoomId, RoomInfo>,
    strapi_client: strapi::Client,
}

pub async fn event_loop(
    bot_id: UserId,
    client: HttpsClient,
    admin_users: Vec<String>,
    strapi_client: strapi::Client,
) -> anyhow::Result<()> {
    let initial_sync_response = client.request(sync_events::Request::new()).await?;
    log::trace!("Initial Sync: {:#?}", initial_sync_response);

    let mut bot_state = State {
        client,
        bot_id,
        admin_users,
        all_room_info: HashMap::new(),
        strapi_client,
    };

    // Handle pending invitations on first sync.
    for (room_id, invitation) in initial_sync_response.rooms.invite {
        handle_invitation(&bot_state.client, room_id, invitation).await;
    }

    // Collect additional room information such as room names and canonical aliases.
    for (room_id, room) in initial_sync_response.rooms.join {
        handle_state(&mut bot_state, &room_id, room.state.events).await;
    }

    let mut sync_stream = Box::pin(bot_state.client.sync(
        None,
        initial_sync_response.next_batch,
        PresenceState::Online,
        Some(Duration::from_secs(30)),
    ));

    while let Some(res) = sync_stream.try_next().await? {
        log::trace!("Response: {:#?}", res);

        // Immediately accept new room invitations.
        for (room_id, invitation) in res.rooms.invite {
            handle_invitation(&bot_state.client, room_id, invitation).await;
        }

        // Only look at rooms the user hasn't left yet
        for (room_id, room) in res.rooms.join {
            handle_state(&mut bot_state, &room_id, room.state.events).await;

            handle_timeline(&mut bot_state, &room_id, room.timeline.events).await;
        }
    }

    Ok(())
}

async fn handle_invitation(client: &HttpsClient, room_id: RoomId, invitation: InvitedRoom) {
    log::info!("Joining '{}' by invitation", room_id.as_str());
    if let Err(_) = client
        .request(join_room_by_id::Request::new(&room_id))
        .await
    {
        log::error!(
            "Failed to respond to invitation. Room ID: {:?}, Invitation: {:?}",
            room_id,
            invitation
        );
    }
}

async fn handle_state(
    bot_state: &mut State,
    room_id: &RoomId,
    events: Vec<ruma::Raw<AnySyncStateEvent>>,
) {
    let real_entry = bot_state
        .all_room_info
        .entry(room_id.clone())
        .or_insert_with(|| RoomInfo {
            id: room_id.as_str().into(),
            name: None,
            alias: None,
        });
    let mut entry = real_entry.clone();

    for event in events.into_iter().flat_map(|r| r.deserialize()) {
        handle_statechange(bot_state, &mut entry, room_id, event).await
    }

    bot_state.all_room_info.insert(room_id.clone(), entry);
}

async fn handle_statechange(
    bot_state: &State,
    entry: &mut RoomInfo,
    room_id: &RoomId,
    state: AnySyncStateEvent,
) {
    match state {
        AnySyncStateEvent::RoomCanonicalAlias(state) => {
            let alias = state.content.alias.map(|a| a.as_str().to_string());
            log::debug!("(Room: {}) Received canonical alias: {:?}", room_id, alias);
            entry.alias = alias;
        }
        AnySyncStateEvent::RoomName(state) => {
            let name = state.content.name().map(|n| n.to_string());
            log::debug!("(Room: {}) Received name: {:?}", room_id, name);
            entry.name = name;
        }
        AnySyncStateEvent::RoomMember(SyncStateEvent {
            content: member,
            sender,
            ..
        }) => {
            if member.membership == MembershipState::Join {
                log::debug!(
                    "User {} joined channel {}",
                    sender.as_str(),
                    room_id.as_str()
                );
                let sender_s = sender.as_str().to_string();
                if bot_state.admin_users.contains(&sender_s) {
                    log::debug!("An admin user joined. Opping.");

                    let mut users = bot_state
                        .admin_users
                        .iter()
                        .map(|u| UserId::try_from(&u[..]).unwrap())
                        .collect::<Vec<_>>();
                    users.push(bot_state.bot_id.clone());
                    let _ = matrix::op_user(&bot_state.client, room_id, &users).await;
                }
            }
        }
        _ => {}
    }
}

async fn handle_timeline(
    bot_state: &mut State,
    room_id: &RoomId,
    events: Vec<ruma::Raw<AnySyncRoomEvent>>,
) {
    for event in events.into_iter().flat_map(|r| r.deserialize()) {
        log::trace!("Room: {:?}, Event: {:?}", room_id, event);
        let real_entry = bot_state
            .all_room_info
            .entry(room_id.clone())
            .or_insert_with(|| RoomInfo {
                id: room_id.as_str().into(),
                name: None,
                alias: None,
            });
        let mut entry = real_entry.clone();

        match event {
            AnySyncRoomEvent::Message(msg) => {
                // Send all message events to the backend server.
                if let AnySyncMessageEvent::RoomMessage(msg) = msg {
                    if let Err(e) =
                        backend::post(&bot_state.strapi_client, &entry, &room_id, &msg).await
                    {
                        log::error!("Failed to post to the backend. Error: {:?}", e);
                    }

                    if let SyncMessageEvent {
                        content:
                            MessageEventContent::Text(TextMessageEventContent {
                                body: msg_body, ..
                            }),
                        sender,
                        ..
                    } = msg
                    {
                        // Handle commands from room messages
                        if let Err(e) = messages::handle(
                            &bot_state.bot_id,
                            &bot_state.client,
                            &room_id,
                            &sender,
                            &msg_body,
                            &mut bot_state.admin_users,
                        )
                        .await
                        {
                            log::error!("Failed to handle message. Error: {:?}", e);
                        }
                    }
                }
            }
            AnySyncRoomEvent::State(state) => {
                handle_statechange(bot_state, &mut entry, room_id, state).await;
            }
            _ => log::debug!("Unhandled event: {:?}", event),
        }

        bot_state
            .all_room_info
            .insert(room_id.clone(), entry.clone());
    }
}
