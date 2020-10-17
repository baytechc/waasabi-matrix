use std::{
    convert::TryFrom,
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};

use assign::assign;
use futures_util::stream::TryStreamExt as _;
use ruma::{
    api::client::r0::{
        filter::FilterDefinition,
        membership::{
            invite_user::{self, InvitationRecipient},
            join_room_by_id, joined_rooms,
        },
        message::send_message_event,
        sync::sync_events,
    },
    events::{
        room::message::{MessageEventContent, TextMessageEventContent},
        AnyMessageEventContent, AnySyncMessageEvent, AnySyncRoomEvent, SyncMessageEvent,
    },
    presence::PresenceState,
    UserId,
};
use ruma_client::{self, HttpsClient};

fn next_id() -> String {
    static NEXT_ID: AtomicUsize = AtomicUsize::new(1);
    NEXT_ID.fetch_add(1, Ordering::SeqCst).to_string()
}

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
                    println!("{:?} in {:?}: {}", sender, room_id, msg_body);

                    if sender == "@jer:rustch.at" {
                        if msg_body == "!channels" {
                            println!("channel listing request from Jan-Erik in #rustfest-test");
                            let response = client.request(joined_rooms::Request::new()).await?;

                            let rooms = response
                                .joined_rooms
                                .into_iter()
                                .map(|room| room.as_str().to_string())
                                .collect::<Vec<_>>();
                            let msg = rooms.join(", ");

                            client
                                .request(send_message_event::Request::new(
                                    &room_id,
                                    &next_id(),
                                    &AnyMessageEventContent::RoomMessage(
                                        MessageEventContent::Text(TextMessageEventContent {
                                            body: msg.into(),
                                            formatted: None,
                                            relates_to: None,
                                        }),
                                    ),
                                ))
                                .await?;
                        }

                        if msg_body.starts_with("!invite ") {
                            let mut parts = msg_body.split(" ");
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
                }
            }
        }
    }

    Ok(())
}
