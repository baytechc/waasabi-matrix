use std::convert::TryFrom;

use ruma::{
    api::client::r0::{
        alias::get_alias,
        membership::invite_user::{self, InvitationRecipient},
    },
    RoomAliasId, RoomId, UserId,
};
use ruma_client::{self, HttpsClient};

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
