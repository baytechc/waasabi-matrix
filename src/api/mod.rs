use super::matrix;
use std::{convert::TryFrom, net::SocketAddr, sync::Arc};

use hyper::{
    service::{make_service_fn, service_fn},
    Body, Method, Request, Response, Server, StatusCode,
};
use ruma::UserId;
use ruma_client::{self, HttpsClient};
use serde::Deserialize;

struct Config {
    client: HttpsClient,
    admin_users: Vec<String>,
    api_secret: String,
}

static INDEX_PAGE: &str = include_str!("../../index.html");

/// Start up a server to handle API requests
pub async fn server(
    addr: SocketAddr,
    api_secret: String,
    admin_users: Vec<String>,
    client: HttpsClient,
) -> anyhow::Result<(), hyper::Error> {
    let config = Arc::new(Config {
        client,
        admin_users,
        api_secret,
    });

    let make_service = make_service_fn(move |_| {
        let config = Arc::clone(&config);

        async move {
            Ok::<_, hyper::Error>(service_fn(move |req| {
                let config = Arc::clone(&config);
                async move {
                    match (req.method(), req.uri().path()) {
                        (&Method::GET, "/") => {
                            let mut response = Response::new(Body::from(INDEX_PAGE));
                            *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
                            Ok::<_, hyper::Error>(response)
                        }
                        (&Method::POST, "/invite") => match invite(&config, req).await {
                            Ok(resp) => Ok(resp),
                            Err(e) => {
                                log::error!("Failed to invite someone. Error: {:?}", e);
                                let mut response = Response::new(Body::empty());
                                *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
                                Ok::<_, hyper::Error>(response)
                            }
                        },
                        (&Method::POST, "/room") => match create_room(&config, req).await {
                            Ok(resp) => Ok(resp),
                            Err(e) => {
                                log::error!("Failed to create a room. Error: {:?}", e);
                                let mut response = Response::new(Body::empty());
                                *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
                                Ok::<_, hyper::Error>(response)
                            }
                        },
                        _ => {
                            let mut response = Response::new(Body::empty());
                            *response.status_mut() = StatusCode::NOT_FOUND;
                            Ok::<_, hyper::Error>(response)
                        }
                    }
                }
            }))
        }
    });

    let server = Server::bind(&addr).serve(make_service);
    server.await
}

#[derive(Deserialize, Debug)]
struct ApiInviteUser {
    user_id: String,
    room_id: String,
    api_key: String,
}

/// POST /invite
///
/// Handle invitation requests and invite the user to a channel.
async fn invite(
    config: &Config,
    request: Request<hyper::Body>,
) -> anyhow::Result<Response<hyper::Body>> {
    let mut response = Response::new(Body::empty());

    let whole_body = hyper::body::to_bytes(request.into_body()).await?;
    let invitation: ApiInviteUser = serde_json::from_slice(&whole_body)?;
    if invitation.api_key != config.api_secret {
        *response.status_mut() = StatusCode::FORBIDDEN;
        return Ok(response);
    }
    log::info!("Received invite request: {:?}", invitation);

    let room_id = matrix::real_room_id(&config.client, &invitation.room_id).await?;
    matrix::invite_user(&config.client, &room_id, &invitation.user_id).await?;

    *response.body_mut() = Body::from(r#"{"status": "ok" }"#);

    Ok(response)
}

#[derive(Deserialize, Debug)]
struct ApiCreateRoom {
    api_key: String,
    alias: String,
    name: String,
    topic: Option<String>,
}

/// POST /room
///
/// Create a new room
async fn create_room(
    config: &Config,
    request: Request<hyper::Body>,
) -> anyhow::Result<Response<hyper::Body>> {
    let mut response = Response::new(Body::empty());

    let whole_body = hyper::body::to_bytes(request.into_body()).await?;
    let room: ApiCreateRoom = serde_json::from_slice(&whole_body)?;
    if room.api_key != config.api_secret {
        *response.status_mut() = StatusCode::FORBIDDEN;
        return Ok(response);
    }
    log::info!("Received create_room: {:?}", room);

    let invite = config
        .admin_users
        .iter()
        .map(|user| UserId::try_from(&user[..]).unwrap())
        .collect::<Vec<_>>();
    matrix::create_room(
        &config.client,
        &room.alias,
        &room.name,
        room.topic.as_deref(),
        &invite,
    )
    .await?;

    *response.body_mut() = Body::from(r#"{"status": "ok" }"#);

    Ok(response)
}
