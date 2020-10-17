use super::matrix;
use std::{net::SocketAddr, sync::Arc};

use hyper::{
    service::{make_service_fn, service_fn},
    Body, Method, Request, Response, Server, StatusCode,
};
use ruma_client::{self, HttpsClient};
use serde::Deserialize;

pub async fn server(port: u16, client: HttpsClient) -> anyhow::Result<(), hyper::Error> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let client = Arc::new(client);

    let make_service = make_service_fn(move |_| {
        let client = Arc::clone(&client);

        async move {
            Ok::<_, hyper::Error>(service_fn(move |req| {
                let client = Arc::clone(&client);
                async move {
                    match (req.method(), req.uri().path()) {
                        (&Method::POST, "/invite") => invite(client, req).await,
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

#[derive(Deserialize)]
struct ApiInviteUser {
    user_id: String,
    room_id: String,
    api_key: String,
}

/// POST /invite
async fn invite(
    matrix_client: Arc<HttpsClient>,
    request: Request<hyper::Body>,
) -> anyhow::Result<Response<hyper::Body>, hyper::Error> {
    let mut response = Response::new(Body::empty());

    let whole_body = hyper::body::to_bytes(request.into_body()).await?;
    let invitation: ApiInviteUser = serde_json::from_slice(&whole_body).unwrap();
    if invitation.api_key == "secret" {
        let room_id = matrix::real_room_id(&matrix_client, &invitation.room_id)
            .await
            .unwrap();
        matrix::invite_user(&matrix_client, &room_id, &invitation.user_id)
            .await
            .unwrap();

        *response.body_mut() = Body::from(r#"{"status": "ok" }"#);
    } else {
        *response.status_mut() = StatusCode::FORBIDDEN;
    }

    Ok(response)
}
