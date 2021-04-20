use crate::strapi;
use std::collections::HashMap;

use ruma::{
    events::{
        room::message::{MessageEventContent, MessageType, TextMessageEventContent},
        SyncMessageEvent,
    },
    RoomId,
};
use serde::Serialize;
use serde_json::{json, Value as JsonValue};

use super::RoomInfo;

#[derive(Serialize)]
struct Data<'a, T> {
    #[serde(rename = "type")]
    typ: &'a str,
    event: &'a str,

    data: T,
}

#[derive(Serialize)]
struct ChatMessage {
    received_by: String,
    channel_id: String,
    channel_name: Option<String>,
    channel_details: JsonValue,
    sender: String,
    sender_details: Option<JsonValue>,
    message: Option<String>,
    message_details: SyncMessageEvent<MessageEventContent>,
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
        MessageEventContent {
            msgtype: MessageType::Text(TextMessageEventContent { body: msg_body, .. }),
            ..
        } => Some(msg_body.to_string()),
        _ => None,
    };
    let chat_message = ChatMessage {
        received_by: "waasabi-matrix".into(),
        channel_id: room_id.as_str().into(),
        channel_name: room_info.name.clone(),
        channel_details: json!({"alias": room_info.alias}),
        sender: msg.sender.as_str().into(),
        sender_details: None,
        message: msg_txt,
        message_details: msg.clone(),
    };

    let client = client.clone();
    tokio::spawn(async move {
        let data = Data {
            typ: "chat",
            event: "new-message",
            data: chat_message,
        };
        log::debug!(
            "Sending data: {}",
            serde_json::to_string_pretty(&data).unwrap()
        );
        let _ = strapi::post(&client, &client.integrations, &data).await;
    });

    Ok(())
}

/// Act on room changes
pub async fn rooms(
    client: &strapi::Client,
    all_rooms: &HashMap<RoomId, RoomInfo>,
) -> anyhow::Result<()> {
    let rooms = all_rooms.values().cloned().collect::<Vec<_>>();

    let client = client.clone();
    tokio::spawn(async move {
        let data = Data {
            typ: "chat",
            event: "channel-info",
            data: rooms,
        };
        log::debug!(
            "Sending data: {}",
            serde_json::to_string_pretty(&data).unwrap()
        );
        let _ = strapi::post(&client, &client.integrations, &data).await;
    });

    Ok(())
}
