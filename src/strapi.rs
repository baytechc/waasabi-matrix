use anyhow::bail;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};

const BACKEND_BASE: &str = "https://backend.rustfest.global";

/// A client to interact with Strapi
pub struct Client {
    http: reqwest::Client,
    jwt: String,
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

fn url(path: &str) -> String {
    format!("{}/{}", BACKEND_BASE, path)
}

/// Login with an API identifer & password.
///
/// This retrieves a JWT token and returns a client usable for authenticated requests.
pub async fn login(identifier: &str, password: &str) -> anyhow::Result<Client> {
    let http = reqwest::Client::builder()
        .user_agent("ferris-bot/0.1.0")
        .build()?;

    let login = Login {
        identifier,
        password,
    };
    let response = http.post(&url("auth/local")).json(&login).send().await?;
    if response.status() != StatusCode::OK {
        bail!("Failed to login");
    }

    let response: LoginResponse = response.json().await?;
    let jwt = response.jwt;
    Ok(Client { http, jwt })
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
        .post(&url(path))
        .bearer_auth(&client.jwt)
        .json(data)
        .send()
        .await?;
    log::debug!("Response: {:?}", res);

    Ok(())
}
