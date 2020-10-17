use std::convert::TryFrom;
use crate::matrix;

use ruma::{
    api::client::r0::{
        membership::{
            invite_user::{self, InvitationRecipient},
            joined_rooms,
        },
    },
    UserId,
    RoomId,
};
use ruma_client::{self, HttpsClient};

pub async fn handle(client: &HttpsClient, room_id: &RoomId, sender: &UserId, msg: &str) -> anyhow::Result<()> {
    println!("{:?} in {:?}: {}", sender, room_id, msg);

    if sender == "@jer:rustch.at" {
        if msg == "!channels" {
            println!("channel listing request from Jan-Erik in #rustfest-test");
            let response = client.request(joined_rooms::Request::new()).await?;

            let rooms = response
                .joined_rooms
                .into_iter()
                .map(|room| room.as_str().to_string())
                .collect::<Vec<_>>();
            let msg = rooms.join(", ");

            matrix::send_message(&client, &room_id, msg).await?;
        }

        if msg.starts_with("!invite ") {
            let mut parts = msg.split(" ");
            let name = parts.nth(1).unwrap();
            let user_id = UserId::try_from(name).unwrap();
            let recipient = InvitationRecipient::UserId { user_id: &user_id };
            println!("Inviting {} to {}", name, room_id);
            if !name.is_empty() {
                client
                    .request(invite_user::Request::new(&room_id, recipient))
                    .await?;
            }
        }
    }
    Ok(())
}
