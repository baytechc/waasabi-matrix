use std::{
    collections::BTreeMap,
    convert::TryFrom,
    sync::atomic::{AtomicUsize, Ordering},
};

use serde::Deserialize;
use ruma::{
    api::client::r0::{
        alias::get_alias,
        membership::{
            invite_user::{self, InvitationRecipient},
            //joined_rooms,
        },
        message::send_message_event,
        room::{create_room, Visibility},
        state::{send_state_event_for_empty_key, get_state_events_for_empty_key},
    },
    events::{
        room::{
            guest_access::{GuestAccess, GuestAccessEventContent},
            history_visibility::{HistoryVisibility, HistoryVisibilityEventContent},
            join_rules::{JoinRule, JoinRulesEventContent},
            message::{MessageEventContent, TextMessageEventContent},
            power_levels::{NotificationPowerLevels, PowerLevelsEventContent},
        },
        AnyInitialStateEvent, AnyMessageEventContent, AnyStateEventContent, EventType,
        InitialStateEvent,
    },
    RoomAliasId, RoomId, UserId,
};
use ruma_client::{self, HttpsClient};

/// Monotonically increasing counter
fn next_id() -> String {
    static NEXT_ID: AtomicUsize = AtomicUsize::new(1);
    NEXT_ID.fetch_add(1, Ordering::SeqCst).to_string()
}

/// Send a message to a room.
///
/// Sends the message as a unformatted plaintext message.
pub async fn send_message<S: Into<String>>(
    matrix_client: &HttpsClient,
    room_id: &RoomId,
    msg: S,
) -> anyhow::Result<()> {
    matrix_client
        .request(send_message_event::Request::new(
            &room_id,
            &next_id(),
            &AnyMessageEventContent::RoomMessage(MessageEventContent::Text(
                TextMessageEventContent {
                    body: msg.into(),
                    formatted: None,
                    relates_to: None,
                },
            )),
        ))
        .await?;
    Ok(())
}

/// Resolve a room alias to a room ID.
///
/// Parses the room alias from a string.
/// The room alias should be in the form `#roomname:homeserver`.
pub async fn real_room_id(
    matrix_client: &HttpsClient,
    room_alias_id: &str,
) -> anyhow::Result<RoomId> {
    if let Ok(room_id) = RoomId::try_from(room_alias_id) {
        return Ok(room_id);
    }
    let room_alias_id = RoomAliasId::try_from(room_alias_id)?;

    let res = matrix_client
        .request(get_alias::Request::new(&room_alias_id))
        .await?;
    let room_id = res.room_id;
    Ok(room_id)
}

/// Invite a user to a room.
///
/// Parses the user ID from a string.
/// The user ID should be in the form `@name:homeserver`
pub async fn invite_user(
    matrix_client: &HttpsClient,
    room_id: &RoomId,
    user_id: &str,
) -> anyhow::Result<()> {
    let user_id = UserId::try_from(user_id)?;
    let recipient = InvitationRecipient::UserId { user_id: &user_id };
    matrix_client
        .request(invite_user::Request::new(&room_id, recipient))
        .await?;

    Ok(())
}

/// Get the current list of joined rooms.
///
/// Returns a list of room IDs.
//pub async fn joined_rooms(matrix_client: &HttpsClient) -> anyhow::Result<Vec<String>> {
    //let response = matrix_client.request(joined_rooms::Request::new()).await?;

    //let rooms = response
        //.joined_rooms
        //.into_iter()
        //.map(|room| room.as_str().to_string())
        //.collect::<Vec<_>>();
    //Ok(rooms)
//}

pub async fn create_room(
    matrix_client: &HttpsClient,
    alias: &str,
    name: &str,
    topic: Option<&str>,
    invite: &[UserId],
) -> anyhow::Result<RoomId> {
    use AnyInitialStateEvent::*;

    let mut req = create_room::Request::new();
    req.room_alias_name = Some(alias);
    req.name = Some(name);
    req.topic = topic;
    req.visibility = Visibility::Private;
    req.invite = invite;

    let initial_state = &[
        RoomGuestAccess(InitialStateEvent {
            content: GuestAccessEventContent::new(GuestAccess::CanJoin),
            state_key: "".into(),
        }),
        RoomJoinRules(InitialStateEvent {
            content: JoinRulesEventContent::new(JoinRule::Invite),
            state_key: "".into(),
        }),
        RoomHistoryVisibility(InitialStateEvent {
            content: HistoryVisibilityEventContent::new(HistoryVisibility::Shared),
            state_key: "".into(),
        }),
    ];
    req.initial_state = initial_state;

    let response = matrix_client.request(req).await?;
    let room_id = response.room_id;

    //for user in invite {
    //invite_user(matrix_client, &room_id, user.as_str()).await?
    //}

    Ok(room_id)
}

#[derive(Deserialize)]
struct PowerLevelEvents {
    events: BTreeMap<String, u32>,
    users: BTreeMap<String, u32>,
}

pub async fn op_user(
    matrix_client: &HttpsClient,
    room_id: &RoomId,
    user_ids: &[UserId],
) -> anyhow::Result<()> {
    // Get the current power levels.
    let req = get_state_events_for_empty_key::Request::new(room_id, EventType::RoomPowerLevels);
    let resp = matrix_client.request(req).await?;

    let content: PowerLevelEvents = serde_json::from_str(resp.content.get())?;

    let mut user_map = BTreeMap::new();

    // Set old state
    for (user, level) in content.users {
        let user_id = match UserId::try_from(user) {
            Ok(id) => id,
            Err(_) => continue
        };
        user_map.insert(user_id, level.into());
    }

    // Now add the new users
    for user_id in user_ids.iter() {
        user_map.insert(user_id.clone(), 100.into());
    }

    let mut event_map = BTreeMap::new();

    // default state
    event_map.insert(EventType::RoomAvatar, 50.into());
    event_map.insert(EventType::RoomCanonicalAlias, 50.into());
    event_map.insert(EventType::RoomEncrypted, 100.into());
    event_map.insert(EventType::RoomHistoryVisibility, 100.into());
    event_map.insert(EventType::RoomName, 50.into());
    event_map.insert(EventType::RoomPowerLevels, 100.into());
    event_map.insert(EventType::RoomServerAcl, 100.into());
    event_map.insert(EventType::RoomTombstone, 100.into());

    // overwriting with old state
    if let Some(&level) = content.events.get("m.room.avatar") {
        event_map.insert(EventType::RoomAvatar, level.into());
    }
    if let Some(&level) = content.events.get("m.room.canonical_alias") {
        event_map.insert(EventType::RoomCanonicalAlias, level.into());
    }
    if let Some(&level) = content.events.get("m.room.encrypted") {
        event_map.insert(EventType::RoomEncrypted, level.into());
    }
    if let Some(&level) = content.events.get("m.room.history_visibility") {
        event_map.insert(EventType::RoomHistoryVisibility, level.into());
    }
    if let Some(&level) = content.events.get("m.room.name") {
        event_map.insert(EventType::RoomName, level.into());
    }
    if let Some(&level) = content.events.get("m.room.power_levels") {
        event_map.insert(EventType::RoomPowerLevels, level.into());
    }
    if let Some(&level) = content.events.get("m.room.server_acl") {
        event_map.insert(EventType::RoomServerAcl, level.into());
    }
    if let Some(&level) = content.events.get("m.room.tombstone") {
        event_map.insert(EventType::RoomTombstone, level.into());
    }

    let content = AnyStateEventContent::RoomPowerLevels(PowerLevelsEventContent {
        ban: 50.into(),
        events: event_map,
        events_default: 0.into(),
        invite: 50.into(),
        kick: 50.into(),
        redact: 50.into(),
        state_default: 50.into(),
        users_default: 0.into(),
        users: user_map,
        notifications: NotificationPowerLevels { room: 50.into() },
    });
    let req = send_state_event_for_empty_key::Request::new(room_id, &content);
    matrix_client.request(req).await?;

    Ok(())
}
