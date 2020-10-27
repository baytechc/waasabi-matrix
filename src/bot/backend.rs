use crate::strapi;

use ruma::{
    events::{
        room::message::{MessageEventContent, TextMessageEventContent},
        SyncMessageEvent,
    },
    RoomId,
};
use serde::Serialize;
use serde_json::{json, Value as JsonValue};

use super::RoomInfo;

#[derive(Serialize)]
struct ChatMessage<'a> {
    received_by: &'a str,
    channel: &'a str,
    channel_name: Option<&'a str>,
    channel_details: JsonValue,
    sender: &'a str,
    sender_details: Option<JsonValue>,
    message: Option<&'a str>,
    message_details: &'a SyncMessageEvent<MessageEventContent>,
}

/// Post a chat message to the backend.
pub async fn post(
    client: &strapi::Client,
    room_info: &RoomInfo,
    room_id: &RoomId,
    msg: &SyncMessageEvent<MessageEventContent>,
) -> anyhow::Result<()> {
    log::debug!("Posting message from {:?}", room_id);

    let msg_txt = match &msg.content {
        MessageEventContent::Text(TextMessageEventContent { body: msg_body, .. }) => Some(&msg_body[..]),
        _ => None,
    };
    let chat_message = ChatMessage {
        received_by: "ferris-bot".into(),
        channel: room_id.as_str(),
        channel_name: room_info.name.as_deref(),
        channel_details: json!({"alias": room_info.alias}),
        sender: msg.sender.as_str(),
        sender_details: None,
        message: msg_txt,
        message_details: &msg,
    };

    log::debug!("Sending data: {}", serde_json::to_string_pretty(&chat_message).unwrap());

    strapi::post(&client, "chat-messages", &chat_message).await?;

    Ok(())
}
