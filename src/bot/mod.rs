use crate::strapi;
use std::collections::HashMap;
use std::time::Duration;

use futures_util::stream::TryStreamExt as _;
use ruma::{
    api::client::r0::{
        membership::join_room_by_id,
        sync::sync_events::{self, InvitedRoom},
    },
    events::{
        room::message::{MessageEventContent, TextMessageEventContent},
        AnySyncMessageEvent, AnySyncRoomEvent, AnySyncStateEvent, SyncMessageEvent,
    },
    presence::PresenceState,
    RoomId,
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

pub async fn event_loop(
    client: HttpsClient,
    admin_users: Vec<String>,
    strapi_client: strapi::Client,
) -> anyhow::Result<()> {
    let initial_sync_response = client.request(sync_events::Request::new()).await?;
    log::trace!("Initial Sync: {:#?}", initial_sync_response);

    // Handle pending invitations on first sync.
    for (room_id, invitation) in initial_sync_response.rooms.invite {
        handle_invitation(&client, room_id, invitation).await;
    }

    // Collect additional room information such as room names and canonical aliases.
    let mut all_room_info = HashMap::new();
    for (room_id, room) in initial_sync_response.rooms.join {
        handle_state(&mut all_room_info, &room_id, room.state.events);
    }

    let mut sync_stream = Box::pin(client.sync(
        None,
        initial_sync_response.next_batch,
        PresenceState::Online,
        Some(Duration::from_secs(30)),
    ));

    while let Some(res) = sync_stream.try_next().await? {
        log::trace!("Response: {:#?}", res);

        // Immediately accept new room invitations.
        for (room_id, invitation) in res.rooms.invite {
            handle_invitation(&client, room_id, invitation).await;
        }

        // Only look at rooms the user hasn't left yet
        for (room_id, room) in res.rooms.join {
            handle_state(&mut all_room_info, &room_id, room.state.events);

            handle_timeline(
                &mut all_room_info,
                &client,
                &strapi_client,
                &admin_users,
                &room_id,
                room.timeline.events,
            )
            .await;
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

fn handle_state(
    all_room_info: &mut HashMap<RoomId, RoomInfo>,
    room_id: &RoomId,
    events: Vec<ruma::Raw<AnySyncStateEvent>>,
) {
    let entry = all_room_info
        .entry(room_id.clone())
        .or_insert_with(|| RoomInfo {
            id: room_id.as_str().into(),
            name: None,
            alias: None,
        });

    for event in events.into_iter().flat_map(|r| r.deserialize()) {
        handle_statechange(entry, room_id, event)
    }
}

fn handle_statechange(
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
        _ => {}
    }
}

async fn handle_timeline(
    all_room_info: &mut HashMap<RoomId, RoomInfo>,
    client: &HttpsClient,
    strapi_client: &strapi::Client,
    admin_users: &[String],
    room_id: &RoomId,
    events: Vec<ruma::Raw<AnySyncRoomEvent>>,
) {
    let entry = all_room_info
        .entry(room_id.clone())
        .or_insert_with(|| RoomInfo {
            id: room_id.as_str().into(),
            name: None,
            alias: None,
        });

    for event in events.into_iter().flat_map(|r| r.deserialize()) {
        log::trace!("Room: {:?}, Event: {:?}", room_id, event);

        match event {
            AnySyncRoomEvent::Message(msg) => {
                // Send all message events to the backend server.
                if let AnySyncMessageEvent::RoomMessage(msg) = msg {
                    if let Err(e) = backend::post(&strapi_client, &entry, &room_id, &msg).await {
                        log::error!("Failed to post to the backend. Error: {:?}", e);
                    }

                    if let SyncMessageEvent {
                        content: MessageEventContent::Text(TextMessageEventContent { body: msg_body, .. }),
                        sender,
                        ..
                    } = msg
                    {
                        // Handle commands from room messages
                        if let Err(_) =
                            messages::handle(&client, &room_id, &sender, &msg_body, &admin_users).await
                        {
                            log::error!("Failed to handle message.");
                        }
                    }
                }
            }
            AnySyncRoomEvent::State(state) => handle_statechange(entry, room_id, state),
            _ => log::debug!("Unhandled event: {:?}", event),
        }
    }
}
