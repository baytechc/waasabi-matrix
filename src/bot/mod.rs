use std::collections::HashMap;
use std::time::Duration;

use futures_util::stream::TryStreamExt as _;
use ruma::{
    api::client::r0::{membership::join_room_by_id, sync::sync_events},
    events::{
        room::message::{MessageEventContent, TextMessageEventContent},
        AnySyncMessageEvent, AnySyncRoomEvent, AnySyncStateEvent, SyncMessageEvent,
    },
    presence::PresenceState,
};
use ruma_client::{self, HttpsClient};
use serde::Serialize;

mod messages;

#[derive(Serialize)]
struct Message {
    room: String,
    user: String,
    message: String,
}

#[derive(Clone, Debug, Serialize)]
struct RoomInfo {
    id: String,
    name: Option<String>,
    alias: Option<String>,
}

#[derive(Serialize)]
struct StrapiEvent<'a> {
    room: RoomInfo,
    data: &'a SyncMessageEvent<MessageEventContent>,
}

pub async fn event_loop(client: HttpsClient) -> anyhow::Result<()> {
    let initial_sync_response = client.request(sync_events::Request::new()).await?;
    log::trace!("Initial Sync: {:#?}", initial_sync_response);

    for (room_id, invitation) in initial_sync_response.rooms.invite {
        log::info!("Joining '{}' by invitation", room_id.as_str());
        if let Err(_) = client
            .request(join_room_by_id::Request::new(&room_id))
            .await
        {
            log::error!(
                "Failed to respond to invitation. Room ID: {:?}, Invitation: {:?}",
                room_id,
                invitation
            );
        }
    }

    let mut all_room_info = HashMap::new();
    for (room_id, room) in initial_sync_response.rooms.join {
        let entry = all_room_info
            .entry(room_id.clone())
            .or_insert_with(|| RoomInfo {
                id: room_id.as_str().into(),
                name: None,
                alias: None,
            });

        for event in room.state.events.into_iter().flat_map(|r| r.deserialize()) {
            match event {
                AnySyncStateEvent::RoomCanonicalAlias(state) => {
                    let alias = state.content.alias.map(|a| a.as_str().to_string());
                    entry.alias = alias;
                }
                AnySyncStateEvent::RoomName(state) => {
                    let name = state.content.name().map(|n| n.to_string());
                    entry.name = name;
                }
                _ => {}
            }
        }
    }

    let mut sync_stream = Box::pin(client.sync(
        None,
        initial_sync_response.next_batch,
        PresenceState::Online,
        Some(Duration::from_secs(30)),
    ));

    let http = reqwest::Client::new();

    while let Some(res) = sync_stream.try_next().await? {
        log::trace!("Response: {:#?}", res);

        for (room_id, invitation) in res.rooms.invite {
            log::info!("Joining '{}' by invitation", room_id.as_str());
            if let Err(_) = client
                .request(join_room_by_id::Request::new(&room_id))
                .await
            {
                log::error!(
                    "Failed to respond to invitation. Room ID: {:?}, Invitation: {:?}",
                    room_id,
                    invitation
                );
            }
        }

        // Only look at rooms the user hasn't left yet
        for (room_id, room) in res.rooms.join {
            for event in room
                .timeline
                .events
                .into_iter()
                .flat_map(|r| r.deserialize())
            {
                log::trace!("Room: {:?}, Event: {:?}", room_id, event);

                if let AnySyncRoomEvent::Message(AnySyncMessageEvent::RoomMessage(msg)) = &event {
                    let room_info = all_room_info
                        .get(&room_id)
                        .map(Clone::clone)
                        .unwrap_or_else(|| RoomInfo {
                            id: room_id.as_str().into(),
                            name: None,
                            alias: None,
                        });
                    let data = StrapiEvent {
                        room: room_info,
                        data: msg,
                    };
                    let _resp = http
                        .post("http://fnordig.de:5678/echo")
                        .json(&data)
                        .send()
                        .await?;
                }

                if let AnySyncRoomEvent::Message(AnySyncMessageEvent::RoomMessage(
                    SyncMessageEvent {
                        content:
                            MessageEventContent::Text(TextMessageEventContent {
                                body: msg_body, ..
                            }),
                        sender,
                        ..
                    },
                )) = event
                {
                    if let Err(_) = messages::handle(&client, &room_id, &sender, &msg_body).await {
                        log::error!("Failed to handle message.");
                    }
                }
            }
        }
    }

    Ok(())
}
