use std::collections::HashMap;
use crate::matrix;

use ruma::{RoomId, UserId};
use ruma_client::{self, HttpsClient};

/// Act on room messages
pub async fn handle(
    client: &HttpsClient,
    room_id: &RoomId,
    sender: &UserId,
    msg: &str,
) -> anyhow::Result<()> {
    println!("({}) <{}> {}", room_id.as_str(), sender.localpart(), msg);

    if sender == "@jer:rustch.at" {
        if msg == "!channels" {
            println!("channel listing request from Jan-Erik in #rustfest-test");
            let rooms = matrix::joined_rooms(client).await?;
            let msg = rooms.join(", ");
            matrix::send_message(&client, &room_id, msg).await?;
        }

        if msg.starts_with("!invite ") {
            let mut parts = msg.split(" ");
            let name = parts.nth(1).unwrap();
            println!("Inviting {} to {}", name, room_id);
            if !name.is_empty() {
                matrix::invite_user(client, &room_id, name).await?
            }
        }
    }
    Ok(())
}
