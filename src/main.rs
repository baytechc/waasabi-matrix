use std::{
    env,
    process::exit,
    time::Duration,
    sync::{
        Arc,
        atomic::{
            AtomicUsize,
            Ordering
        },
    },
    convert::TryFrom,
    net::SocketAddr,
};

use hyper::{
    Body,
    Method,
    Response,
    Server,
    StatusCode,
    service::{
        make_service_fn,
        service_fn,
    },
};
use serde::Deserialize;
use assign::assign;
use futures_util::stream::TryStreamExt as _;
use futures_util::future;
use http::Uri;
use ruma::{
    api::client::r0::{
        filter::FilterDefinition,
        sync::sync_events,
        message::send_message_event,
        membership::{
            join_room_by_id,
            joined_rooms,
            invite_user::{self, InvitationRecipient},
        },
        alias::get_alias,
    },
    events::{
        room::message::{MessageEventContent, TextMessageEventContent},
        AnySyncMessageEvent, AnySyncRoomEvent, SyncMessageEvent,
        AnyMessageEventContent,
    },
    presence::PresenceState,
    UserId,
    RoomAliasId,

};
use ruma_client::{self, HttpsClient};

fn next_id() -> String {
    static NEXT_ID: AtomicUsize = AtomicUsize::new(1);
    NEXT_ID.fetch_add(1, Ordering::SeqCst).to_string()
}

#[derive(Deserialize)]
struct ApiInviteUser {
    user_id: String,
    room_id: String,
    api_key: String,
}

async fn matrix_bot(homeserver_url: Uri, username: &str, password: &str) -> anyhow::Result<()> {
    let client = HttpsClient::https(homeserver_url, None);

    client.log_in(username, password, None, None).await?;
    let bot = event_loop(client.clone());

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));

    let client = Arc::new(client);
    let make_service = make_service_fn(move |_| {
        let client = Arc::clone(&client);

        async move {
            Ok::<_, hyper::Error>(service_fn(move |req| {
                let client = Arc::clone(&client);
                async move {
                    let mut response = Response::new(Body::empty());

                    match (req.method(), req.uri().path()) {
                        (&Method::POST, "/invite") => {
                            let whole_body = hyper::body::to_bytes(req.into_body()).await?;
                            let invitation: ApiInviteUser = serde_json::from_slice(&whole_body).unwrap();
                            if invitation.api_key == "secret" {
                                let user_id = UserId::try_from(&*invitation.user_id).unwrap();
                                let room_alias_id = RoomAliasId::try_from(&*invitation.room_id).unwrap();
                                let res = client.request(get_alias::Request::new(&room_alias_id)).await.unwrap();
                                let room_id = res.room_id;
                                let recipient = InvitationRecipient::UserId { user_id: &user_id };
                                client.request(invite_user::Request::new(&room_id, recipient)).await.unwrap();
                                *response.body_mut() = Body::from(r#"{"status": "ok" }"#);
                            } else {
                                *response.status_mut() = StatusCode::FORBIDDEN;
                            }
                        },
                        _ => {
                            *response.status_mut() = StatusCode::NOT_FOUND;
                        },
                    };

                    Ok::<_, hyper::Error>(response)
                }
            }))
        }
    });

    let server = Server::bind(&addr).serve(make_service);

    let (bot_ended, server_ended) = future::join(bot, server).await;
    bot_ended?;
    server_ended?;

    Ok(())
}

async fn event_loop(client: HttpsClient) -> anyhow::Result<()> {
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
            client.request(join_room_by_id::Request::new(&room_id)).await?;
        }

        // Only look at rooms the user hasn't left yet
        for (room_id, room) in res.rooms.join {
            for event in room.timeline.events.into_iter().flat_map(|r| r.deserialize()) {
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
                            let response = client
                                .request(joined_rooms::Request::new())
                                .await?;

                            let rooms = response.joined_rooms.into_iter().map(|room| room.as_str().to_string()).collect::<Vec<_>>();
                            let msg = rooms.join(", ");

                            client
                                .request(send_message_event::Request::new(
                                        &room_id,
                                        &next_id(),
                                        &AnyMessageEventContent::RoomMessage(MessageEventContent::Text(
                                                TextMessageEventContent {
                                                    body: msg.into(),
                                                    formatted: None,
                                                    relates_to: None,
                                                },
                                        )),
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
                                client.request(invite_user::Request::new(&room_id, recipient)).await?;
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let (homeserver_url, username, password) =
        match (env::args().nth(1), env::args().nth(2), env::var("MATRIX_PASSWORD")) {
            (Some(a), Some(b), Ok(c)) => (a, b, c),
            _ => {
                eprintln!(
                    "Usage: {} <homeserver_url> <username>",
                    env::args().next().unwrap()
                );
                exit(1)
            }
        };

    let server = homeserver_url.parse()?;
    matrix_bot(server, &username, &password).await
}
