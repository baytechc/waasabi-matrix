use std::{
    convert::TryFrom,
    sync::atomic::{AtomicUsize, Ordering},
};

use ruma::{
    api::client::r0::{
        alias::get_alias,
        membership::{
            invite_user::{self, InvitationRecipient},
            joined_rooms,
        },
        message::send_message_event,
    },
    events::{
        room::message::{MessageEventContent, TextMessageEventContent},
        AnyMessageEventContent,
    },
    RoomAliasId, RoomId, UserId,
};
use ruma_client::{self, HttpsClient};

/// Monotonically increasing counter
fn next_id() -> String {
    static NEXT_ID: AtomicUsize = AtomicUsize::new(1);
    NEXT_ID.fetch_add(1, Ordering::SeqCst).to_string()
}

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

pub async fn real_room_id(
    matrix_client: &HttpsClient,
    room_alias_id: &str,
) -> anyhow::Result<RoomId> {
    let room_alias_id = RoomAliasId::try_from(room_alias_id)?;

    let res = matrix_client
        .request(get_alias::Request::new(&room_alias_id))
        .await?;
    let room_id = res.room_id;
    Ok(room_id)
}

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

pub async fn joined_rooms(matrix_client: &HttpsClient) -> anyhow::Result<Vec<String>> {
    let response = matrix_client.request(joined_rooms::Request::new()).await?;

    let rooms = response
        .joined_rooms
        .into_iter()
        .map(|room| room.as_str().to_string())
        .collect::<Vec<_>>();
    Ok(rooms)
}
