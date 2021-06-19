//! Handle individual messages
//!
//! Messages might contain commands to run.

use crate::matrix;
use std::convert::TryFrom;

use ruma::{RoomId, UserId};
use ruma_client;
type Client = ruma_client::Client<ruma_client::http_client::HyperNativeTls>;

enum Command {
    /// Ping-pong with the bot
    Ping,
    /// Invite a user to the current room
    Invite(Vec<String>),
    /// Ask for the current list of admins
    OpAsk,
    /// Give admin permissions to all admin users or a specified one
    Op(Vec<String>),
    /// Create a new room
    Create(Vec<String>),
}

impl TryFrom<(&'_ str, Vec<String>)> for Command {
    type Error = anyhow::Error;

    fn try_from((cmd, args): (&str, Vec<String>)) -> Result<Self, Self::Error> {
        let cmd = match (&*cmd, args.len()) {
            ("!ping", 0) => Command::Ping,
            ("!invite", 1) => Command::Invite(args),
            ("?op", 0) => Command::OpAsk,
            ("!op", _) => Command::Op(args),
            ("!create", _) => Command::Create(args),
            _ => anyhow::bail!("invalid command"),
        };

        Ok(cmd)
    }
}

/// Act on room messages
pub async fn handle(
    bot_id: &UserId,
    client: &Client,
    room_id: &RoomId,
    sender: &UserId,
    msg: &str,
    admin_users: &mut Vec<String>,
) -> anyhow::Result<()> {
    log::trace!("({}) <{}> {}", room_id.as_str(), sender.localpart(), msg);

    // Only admin users can run commands currently.
    if !admin_users.contains(&sender.as_str().to_string()) {
        return Ok(());
    }

    let mut parts = msg.split(' ');
    let cmd = match parts.next() {
        Some(cmd) => cmd,
        None => return Ok(()),
    };
    let args = parts.map(str::to_owned).collect::<Vec<_>>();

    let cmd = Command::try_from((cmd, args))?;

    match cmd {
        Command::Ping => ping(client, room_id).await?,
        Command::Invite(args) => invite(client, room_id, &args).await?,
        Command::OpAsk => op_ask(client, room_id, admin_users).await?,
        Command::Op(args) => op(client, room_id, bot_id, admin_users, &args).await?,
        Command::Create(args) => create(client, room_id, admin_users, &args).await?,
    }

    Ok(())
}

async fn ping(client: &Client, room_id: &RoomId) -> anyhow::Result<()> {
    matrix::send_message(&client, &room_id, "PONG!").await?;
    Ok(())
}

async fn invite(client: &Client, room_id: &RoomId, args: &[String]) -> anyhow::Result<()> {
    let name = &args[0];
    println!("Inviting {} to {}", name, room_id);
    if !name.is_empty() {
        matrix::invite_user(client, &room_id, name).await?
    }

    Ok(())
}

async fn op_ask(client: &Client, room_id: &RoomId, admin_users: &[String]) -> anyhow::Result<()> {
    let users = admin_users.join(", ");
    let msg = format!("Current admins: {}", users);
    matrix::send_message(&client, &room_id, msg).await?;
    Ok(())
}

async fn op(
    client: &Client,
    room_id: &RoomId,
    bot_id: &UserId,
    admin_users: &mut Vec<String>,
    args: &[String],
) -> anyhow::Result<()> {
    if args.len() > 1 {
        let msg = "Invalid. Require no or one argument.";
        matrix::send_message(&client, &room_id, msg).await?;
        return Ok(());
    }

    if !args.is_empty() {
        let user = args[0].to_string();
        let msg = format!("Added {}", user);
        admin_users.push(user);
        matrix::send_message(&client, &room_id, msg).await?;
    }

    let mut users = admin_users
        .iter()
        .map(|u| UserId::try_from(&u[..]).unwrap())
        .collect::<Vec<_>>();
    users.push(bot_id.clone());

    let _ = matrix::op_user(&client, room_id, &users).await;
    Ok(())
}

async fn create(
    client: &Client,
    room_id: &RoomId,
    admin_users: &mut Vec<String>,
    args: &[String],
) -> anyhow::Result<()> {
    if args.len() != 2 {
        matrix::send_message(
            &client,
            &room_id,
            "Need arguments: <room alias> <room name>",
        )
        .await?;
    } else {
        let alias = &args[0];
        let name = &args[1];
        let invites = admin_users
            .iter()
            .map(|u| UserId::try_from(&u[..]).unwrap())
            .collect::<Vec<_>>();
        let msg = format!(
            "Will create a room named #{}:rustch.at with the name: {}. You will be invited.",
            alias, name
        );
        matrix::send_message(&client, &room_id, msg).await?;
        matrix::create_room(client, &alias, &name, None, &invites).await?;
    }

    Ok(())
}
