//! Minimal sample implementation of the strapi-compatible backend.
//!
//! It allows any login and simply logs all other requests.

use hyper::{
    service::{make_service_fn, service_fn},
    Body, Method, Response, Server, StatusCode,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    server().await?;
    Ok(())
}

/// Start up a server to handle API requests
pub async fn server() -> anyhow::Result<(), hyper::Error> {
    let make_service = make_service_fn(move |_| async move {
        Ok::<_, hyper::Error>(service_fn(move |req| async move {
            log::info!("{:?} {}", req.method(), req.uri().path());

            match (req.method(), req.uri().path()) {
                (&Method::POST, "/auth/local") => {
                    let mut response = Response::new(Body::empty());
                    *response.status_mut() = StatusCode::OK;
                    *response.body_mut() =
                        Body::from(r#"{"jwt": "b026b916-736a-4dba-90cc-23b49c847c88" }"#);
                    Ok::<_, hyper::Error>(response)
                }
                _ => {
                    let body = hyper::body::to_bytes(req.into_body()).await?;
                    let body_s = String::from_utf8_lossy(&body);
                    if body_s.len() > 0 {
                        log::info!("Body:\n---\n{}\n---\n", body_s);
                    }

                    let mut response = Response::new(Body::empty());
                    *response.status_mut() = StatusCode::OK;
                    Ok::<_, hyper::Error>(response)
                }
            }
        }))
    });

    let addr = ([127, 0, 0, 1], 3300).into();
    let server = Server::bind(&addr).serve(make_service);
    server.await
}
