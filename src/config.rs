use std::{fs, net::SocketAddr, path::Path};

use anyhow::Result;
use http::Uri;
use serde::{de, Deserialize};

#[derive(Deserialize)]
pub struct Configuration {
    /// Configuration for the Matrix server.
    pub matrix: MatrixConfig,

    /// Configuration for the HTTP API.
    pub api: ApiConfig,

    /// Configuration for the backend
    pub backend: BackendConfig,
}

#[derive(Deserialize)]
pub struct MatrixConfig {
    /// The bot's homeserver.
    #[serde(deserialize_with = "deserialize_uri")]
    pub homeserver: Uri,

    /// The bot's account name.
    pub user: String,

    /// The bot's password.
    pub password: String,

    /// List of Matrix accounts with admin privileges for this bot.
    pub admins: Vec<String>,
}

#[derive(Deserialize)]
pub struct ApiConfig {
    /// The host and port to listen on.
    pub listen: SocketAddr,

    /// The API secret.
    pub secret: String,
}

#[derive(Deserialize)]
pub struct BackendConfig {
    pub host: String,
    pub user: String,
    pub password: String,
}

/// Read the configuration from the provided file.
pub fn parse<P: AsRef<Path>>(file: P) -> Result<Configuration> {
    let content = fs::read_to_string(file)?;
    let cfg = toml::from_str(&content)?;
    Ok(cfg)
}

fn deserialize_uri<'de, D>(deserializer: D) -> Result<Uri, D::Error>
where
    D: de::Deserializer<'de>,
{
    let uri: &str = de::Deserialize::deserialize(deserializer)?;
    uri.parse().map_err(de::Error::custom)
}
