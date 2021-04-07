//! Main bot logic
//!
//! This is the main event loop of the bot.
//! It waits for messages from the server, updates its internal state about rooms,
//! reacts to invitations and commands and relays received messages.

use crate::{matrix, strapi};
use std::collections::{BTreeMap, HashMap};
use std::convert::TryFrom;
use std::time::Duration;

use futures_util::stream::TryStreamExt as _;
use ruma::{
    api::client::r0::{
        membership::join_room_by_id,
        sync::sync_events::{self, InvitedRoom, JoinedRoom},
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

#[derive(Clone, Debug, Serialize, Default)]
pub struct RoomInfo {
    id: String,
    name: Option<String>,
    alias: Option<String>,
    topic: Option<String>,
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

    let mut pending_invites: HashMap<RoomId, usize> = HashMap::new();

    // Handle pending invitations on first sync.
    let mut state_change = handle_invites(
        initial_sync_response.rooms.invite,
        &bot_state.client,
        &mut bot_state.all_room_info,
        &mut pending_invites,
    )
    .await;

    // Collect additional room information such as room names and canonical aliases.
    state_change |= handle_joined_rooms(initial_sync_response.rooms.join, &mut bot_state).await;

    if state_change {
        if let Err(e) = backend::rooms(&bot_state.strapi_client, &bot_state.all_room_info).await {
            log::error!("Failed to post room changes to the backend. Error: {:?}", e);
        }
    }

    let mut sync_stream = Box::pin(bot_state.client.sync(
        None,
        initial_sync_response.next_batch,
        PresenceState::Online,
        Some(Duration::from_secs(30)),
    ));

    while let Some(res) = sync_stream.try_next().await? {
        log::trace!("Response: {:#?}", res);
        let mut state_change = false;

        state_change |= handle_pending_invites(
            &mut pending_invites,
            &bot_state.client,
            &mut bot_state.all_room_info,
        )
        .await;

        // Immediately accept new room invitations.
        state_change |= handle_invites(
            res.rooms.invite,
            &bot_state.client,
            &mut bot_state.all_room_info,
            &mut pending_invites,
        )
        .await;

        // Only look at rooms the user hasn't left yet
        state_change |= handle_joined_rooms(res.rooms.join, &mut bot_state).await;

        if state_change {
            if let Err(e) = backend::rooms(&bot_state.strapi_client, &bot_state.all_room_info).await
            {
                log::error!("Failed to post room changes to the backend. Error: {:?}", e);
            }
        }
    }

    Ok(())
}

async fn handle_invites(
    invites: BTreeMap<RoomId, InvitedRoom>,
    client: &HttpsClient,
    all_room_info: &mut HashMap<RoomId, RoomInfo>,
    pending_invites: &mut HashMap<RoomId, usize>,
) -> bool {
    let mut state_change = false;

    for (room_id, invitation) in invites {
        let invite_resp =
            handle_invitation(client, room_id.clone(), Some(invitation), all_room_info).await;

        if invite_resp.is_err() {
            pending_invites.insert(room_id, 3);
        } else {
            state_change = true;
        }
    }

    state_change
}

async fn handle_pending_invites(
    pending_invites: &mut HashMap<RoomId, usize>,
    client: &HttpsClient,
    all_room_info: &mut HashMap<RoomId, RoomInfo>,
) -> bool {
    let mut state_change = false;
    let mut to_delete = vec![];
    for (room_id, tries_left) in pending_invites.iter_mut() {
        *tries_left -= 1;

        let invite_resp = handle_invitation(client, room_id.clone(), None, all_room_info).await;
        if invite_resp.is_ok() {
            to_delete.push(room_id.clone());
            state_change = true;
        } else if *tries_left == 0 {
            to_delete.push(room_id.clone());
        }
    }

    for room_id in to_delete {
        pending_invites.remove(&room_id);
    }

    state_change
}

async fn handle_joined_rooms(rooms: BTreeMap<RoomId, JoinedRoom>, bot_state: &mut State) -> bool {
    let mut state_change = false;
    for (room_id, room) in rooms {
        state_change |= handle_state(bot_state, &room_id, room.state.events).await;
        state_change |= handle_timeline(bot_state, &room_id, room.timeline.events, false).await;
    }
    state_change
}

async fn handle_invitation(
    client: &HttpsClient,
    room_id: RoomId,
    invitation: Option<InvitedRoom>,
    all_room_info: &mut HashMap<RoomId, RoomInfo>,
) -> anyhow::Result<()> {
    log::info!("Joining '{}' by invitation", room_id.as_str());
    if let Err(e) = client
        .request(join_room_by_id::Request::new(&room_id))
        .await
    {
        log::error!(
            "Failed to respond to invitation. Room ID: {:?}, Invitation: {:?}\nError: {:?}",
            room_id.as_str(),
            invitation,
            e
        );
        return Err(e.into());
    }

    let _entry = all_room_info
        .entry(room_id.clone())
        .or_insert_with(|| RoomInfo {
            id: room_id.as_str().into(),
            ..Default::default()
        });
    Ok(())
}

async fn handle_state(
    bot_state: &mut State,
    room_id: &RoomId,
    events: Vec<ruma::Raw<AnySyncStateEvent>>,
) -> bool {
    let real_entry = bot_state
        .all_room_info
        .entry(room_id.clone())
        .or_insert_with(|| RoomInfo {
            id: room_id.as_str().into(),
            ..Default::default()
        });
    let mut entry = real_entry.clone();

    let mut state = false;
    for event in events.into_iter().flat_map(|r| r.deserialize()) {
        state |= handle_statechange(bot_state, &mut entry, room_id, event).await;
    }

    bot_state.all_room_info.insert(room_id.clone(), entry);
    state
}

async fn handle_statechange(
    bot_state: &State,
    entry: &mut RoomInfo,
    room_id: &RoomId,
    state: AnySyncStateEvent,
) -> bool {
    match state {
        AnySyncStateEvent::RoomCanonicalAlias(state) => {
            let alias = state.content.alias.map(|a| a.as_str().to_string());
            log::debug!("(Room: {}) Received canonical alias: {:?}", room_id, alias);
            entry.alias = alias;
            true
        }
        AnySyncStateEvent::RoomName(state) => {
            let name = state.content.name().map(|n| n.to_string());
            log::debug!("(Room: {}) Received name: {:?}", room_id, name);
            entry.name = name;
            true
        }
        AnySyncStateEvent::RoomTopic(state) => {
            let topic = state.content.topic;
            log::debug!("(Room: {}) Received topic: {:?}", room_id, topic);
            entry.topic = Some(topic);
            true
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
            false
        }
        state => {
            log::debug!("Unhandled state: {:?}", state);
            false
        }
    }
}

async fn handle_timeline(
    bot_state: &mut State,
    room_id: &RoomId,
    events: Vec<ruma::Raw<AnySyncRoomEvent>>,
    handle_messages: bool,
) -> bool {
    let mut roomstate = false;

    for event in events.into_iter().flat_map(|r| r.deserialize()) {
        log::trace!("Room: {:?}, Event: {:?}", room_id, event);
        let real_entry = bot_state
            .all_room_info
            .entry(room_id.clone())
            .or_insert_with(|| RoomInfo {
                id: room_id.as_str().into(),
                ..Default::default()
            });
        let mut entry = real_entry.clone();

        match event {
            AnySyncRoomEvent::Message(msg) if handle_messages => {
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
                roomstate |= handle_statechange(bot_state, &mut entry, room_id, state).await;
            }
            _ => log::debug!("Unhandled event: {:?}", event),
        }

        bot_state
            .all_room_info
            .insert(room_id.clone(), entry.clone());
    }
    roomstate
}
