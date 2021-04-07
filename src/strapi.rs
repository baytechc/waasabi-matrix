//! Simple Strapi client implementation.
//!
//! Strapi is the currently used backend, storing data about the event
//! and acting as the hub between the Matrix chat and the event frontend.

use anyhow::bail;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};

/// A client to interact with Strapi
#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    jwt: String,
    base: String,
    pub integrations: String,
}

#[derive(Serialize)]
struct Login<'a> {
    identifier: &'a str,
    password: &'a str,
}

#[derive(Deserialize)]
struct LoginResponse {
    jwt: String,
}

fn _url(base: &str, path: &str) -> String {
    format!("{}/{}", base, path)
}

impl Client {
    fn url(&self, path: &str) -> String {
        _url(&self.base, path)
    }
}

/// Login with an API identifer & password.
///
/// This retrieves a JWT token and returns a client usable for authenticated requests.
pub async fn login(
    base: &str,
    integrations: &str,
    identifier: &str,
    password: &str,
) -> anyhow::Result<Client> {
    let http = reqwest::Client::builder()
        .user_agent("ferris-bot/0.1.0")
        .build()?;

    let login = Login {
        identifier,
        password,
    };
    let response = http
        .post(&_url(base, "auth/local"))
        .json(&login)
        .send()
        .await?;
    if response.status() != StatusCode::OK {
        bail!("Failed to login, status: {:?}", response.status());
    }

    let response: LoginResponse = response.json().await?;
    let jwt = response.jwt;
    Ok(Client {
        http,
        jwt,
        base: base.to_string(),
        integrations: integrations.to_string(),
    })
}

/// Post to the API with an authorized client.
pub async fn post<T: Serialize + ?Sized>(
    client: &Client,
    path: &str,
    data: &T,
) -> anyhow::Result<()> {
    log::debug!("JWT: {}", client.jwt);
    let res = client
        .http
        .post(&client.url(path))
        .bearer_auth(&client.jwt)
        .json(data)
        .send()
        .await?;
    log::debug!("Response: {:?}", res);

    Ok(())
}
