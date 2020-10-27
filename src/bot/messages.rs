use std::convert::TryFrom;
use crate::matrix;

use ruma::{RoomId, UserId};
use ruma_client::{self, HttpsClient};

/// Act on room messages
pub async fn handle(
    bot_id: &UserId,
    client: &HttpsClient,
    room_id: &RoomId,
    sender: &UserId,
    msg: &str,
    admin_users: &[String]
) -> anyhow::Result<()> {
    println!("({}) <{}> {}", room_id.as_str(), sender.localpart(), msg);

    if admin_users.contains(&sender.as_str().to_string()) {
        if msg == "!ping" {
            matrix::send_message(&client, &room_id, "PONG!").await?;
        }

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

        if msg.starts_with("!create ") {
            let parts = msg.splitn(3, " ").skip(1).collect::<Vec<_>>();
            if parts.len() != 2 {
                matrix::send_message(&client, &room_id, "Need arguments: <room alias> <room name>").await?;
            } else {
                let alias = &parts[0];
                let name = &parts[1];
                let invites = admin_users.iter().map(|u| {
                    UserId::try_from(&u[..]).unwrap()
                }).collect::<Vec<_>>();
                let msg = format!("Will create a room named #{}:rustch.at with the name: {}. You will be invited.", alias, name);
                matrix::send_message(&client, &room_id, msg).await?;
                matrix::create_room(client, &alias, &name, &invites).await?;
            }
        }

        if msg.starts_with("!op") {
            let mut users = admin_users.iter().map(|u| {
                UserId::try_from(&u[..]).unwrap()
            }).collect::<Vec<_>>();
            users.push(bot_id.clone());

            let _ = matrix::op_user(&client, room_id, &users).await;
        }
    }
    Ok(())
}
