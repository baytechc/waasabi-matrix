use std::{
    time::Duration,
};

use assign::assign;
use futures_util::stream::TryStreamExt as _;
use ruma::{
    api::client::r0::{
        filter::FilterDefinition,
        membership::{
            join_room_by_id,
        },
        sync::sync_events,
    },
    events::{
        room::message::{MessageEventContent, TextMessageEventContent},
        AnySyncMessageEvent, AnySyncRoomEvent, SyncMessageEvent,
    },
    presence::PresenceState,
};
use ruma_client::{self, HttpsClient};

mod messages;

pub async fn event_loop(client: HttpsClient) -> anyhow::Result<()> {
    let initial_sync_response = client
        .request(assign!(sync_events::Request::new(), {
            filter: Some(FilterDefinition::ignore_all().into()),
        }))
        .await?;

    let mut sync_stream = Box::pin(client.sync(
        None,
        initial_sync_response.next_batch,
        PresenceState::Online,
        Some(Duration::from_secs(30)),
    ));

    while let Some(res) = sync_stream.try_next().await? {
        for (room_id, _invitation) in res.rooms.invite {
            println!("Joining '{}' by invitation", room_id.as_str());
            client
                .request(join_room_by_id::Request::new(&room_id))
                .await?;
        }

        // Only look at rooms the user hasn't left yet
        for (room_id, room) in res.rooms.join {
            for event in room
                .timeline
                .events
                .into_iter()
                .flat_map(|r| r.deserialize())
            {
                // Filter out the text messages
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
                    messages::handle(&client, &room_id, &sender, &msg_body).await?;
                }
            }
        }
    }

    Ok(())
}
