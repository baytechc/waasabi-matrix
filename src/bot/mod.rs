//! Main bot logic
//!
//! This is the main logic of the bot.
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

/// The bot's main event loop.
///
/// Continously stream server responses and handle all state changes and messages.
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
        pending_invites: HashMap::new(),
    };

    let next_batch = initial_sync_response.next_batch.clone();
    bot_state.handle_sync(initial_sync_response, false).await;

    let mut sync_stream = Box::pin(bot_state.client.sync(
        None,
        next_batch,
        PresenceState::Online,
        Some(Duration::from_secs(30)),
    ));

    while let Some(res) = sync_stream.try_next().await? {
        bot_state.handle_sync(res, true).await;
    }

    log::info!("Sync stream ended.");

    Ok(())
}

/// A room's known information.
#[derive(Clone, Debug, Serialize, Default)]
pub struct RoomInfo {
    /// The room ID
    id: String,
    /// The room's name, if known.
    name: Option<String>,
    /// The room's canonical alias, if known.
    alias: Option<String>,
    /// The room's topic, if known.
    topic: Option<String>,
}

struct State {
    client: HttpsClient,
    bot_id: UserId,
    admin_users: Vec<String>,
    all_room_info: HashMap<RoomId, RoomInfo>,
    strapi_client: strapi::Client,
    pending_invites: HashMap<RoomId, usize>,
}

impl State {
    /// Handle a sync response.
    ///
    /// This is the main event handler.
    /// It handles invites and all room events, such as messages or state changes.
    async fn handle_sync(&mut self, sync: sync_events::Response, handle_messages: bool) {
        log::trace!("Response: {:#?}", sync);
        let mut state_change = false;

        // Immediately accept new room invitations and retry pending invites.
        state_change |= self.handle_invites(sync.rooms.invite).await;

        // Only look at rooms the user hasn't left yet
        state_change |= self.handle_rooms(sync.rooms.join, handle_messages).await;

        // If any room state changed, relay that information to the backend.
        if state_change {
            if let Err(e) = backend::rooms(&self.strapi_client, &self.all_room_info).await {
                log::error!("Failed to post room changes to the backend. Error: {:?}", e);
            }
        }
    }

    /// React to new invites by trying to join.
    ///
    /// If joining a room fails the invitiation will be retried later, up to 3 times.
    ///
    /// Returns `true` if any room state changed.
    /// Returns `false` otherwise.
    async fn handle_invites(&mut self, invites: BTreeMap<RoomId, InvitedRoom>) -> bool {
        // First insert new invites to be tried.
        for (room_id, _) in invites {
            // 4 = try once immediately, retry up to 3 times.
            self.pending_invites.insert(room_id, 4);
        }

        let mut state_change = false;
        let mut to_delete = vec![];
        for (room_id, tries_left) in self.pending_invites.iter_mut() {
            *tries_left -= 1;

            let invite_resp =
                accept_invitation(&self.client, room_id.clone(), &mut self.all_room_info).await;
            if invite_resp.is_ok() {
                to_delete.push(room_id.clone());
                state_change = true;
            } else if *tries_left == 0 {
                to_delete.push(room_id.clone());
            }
        }

        for room_id in to_delete {
            self.pending_invites.remove(&room_id);
        }

        state_change
    }

    /// Handle incoming state changes and timeline events for joined rooms.
    ///
    /// Returns `true` if any room state changed.
    /// Returns `false` otherwise.
    async fn handle_rooms(
        &mut self,
        rooms: BTreeMap<RoomId, JoinedRoom>,
        handle_messages: bool,
    ) -> bool {
        let mut state_change = false;
        for (room_id, room) in rooms {
            state_change |= handle_room_events(self, &room_id, room.state.events).await;
            state_change |=
                handle_timeline(self, &room_id, room.timeline.events, handle_messages).await;
        }
        state_change
    }
}

/// Join the room by invitiation.
///
/// This updates the room info state.
///
/// Returns `Ok(())` if the room was joined.
/// Returns an error if joining the room failed.
async fn accept_invitation(
    client: &HttpsClient,
    room_id: RoomId,
    all_room_info: &mut HashMap<RoomId, RoomInfo>,
) -> anyhow::Result<()> {
    log::info!("Joining '{}' by invitation", room_id.as_str());
    if let Err(e) = client
        .request(join_room_by_id::Request::new(&room_id))
        .await
    {
        log::error!(
            "Failed to respond to invitation. Room ID: {:?}, \nError: {:?}",
            room_id.as_str(),
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

/// Handle state events within a room.
async fn handle_room_events(
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

/// Handle a state event within the given room.
///
/// This may change the room info state.
/// If new users join the room and they are in the admin user group,
/// they will be given appropriate permissions.
///
/// Returns `true` if any room state changed.
/// Returns `false` otherwise.
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

/// Handle any room event from the timeline.
///
/// This will:
///
/// * Relay room messages to the backend.
/// * Handle any room state change.
///
/// Returns `true` if any room state changed.
/// Returns `false` otherwise.
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
