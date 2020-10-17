use std::{convert::TryFrom, net::SocketAddr, sync::Arc};

use hyper::{
    service::{make_service_fn, service_fn},
    Body, Method, Response, Server, StatusCode,
};
use serde::Deserialize;

use ruma::{
    api::client::r0::{
        alias::get_alias,
        membership::invite_user::{self, InvitationRecipient},
    },
    RoomAliasId, UserId,
};
use ruma_client::{self, HttpsClient};

#[derive(Deserialize)]
struct ApiInviteUser {
    user_id: String,
    room_id: String,
    api_key: String,
}

pub async fn server(port: u16, client: HttpsClient) -> anyhow::Result<(), hyper::Error> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
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
                            let invitation: ApiInviteUser =
                                serde_json::from_slice(&whole_body).unwrap();
                            if invitation.api_key == "secret" {
                                let user_id = UserId::try_from(&*invitation.user_id).unwrap();
                                let room_alias_id =
                                    RoomAliasId::try_from(&*invitation.room_id).unwrap();
                                let res = client
                                    .request(get_alias::Request::new(&room_alias_id))
                                    .await
                                    .unwrap();
                                let room_id = res.room_id;
                                let recipient = InvitationRecipient::UserId { user_id: &user_id };
                                client
                                    .request(invite_user::Request::new(&room_id, recipient))
                                    .await
                                    .unwrap();
                                *response.body_mut() = Body::from(r#"{"status": "ok" }"#);
                            } else {
                                *response.status_mut() = StatusCode::FORBIDDEN;
                            }
                        }
                        _ => {
                            *response.status_mut() = StatusCode::NOT_FOUND;
                        }
                    };

                    Ok::<_, hyper::Error>(response)
                }
            }))
        }
    });

    let server = Server::bind(&addr).serve(make_service);
    server.await
}
