use std::{convert::TryFrom, env, net::SocketAddr};

use futures_util::future;
use http::Uri;
use ruma::{DeviceId, UserId};
use ruma_client::HttpsClient;

mod api;
mod bot;
mod matrix;
mod strapi;

struct Config {
    matrix_homeserver: Uri,
    matrix_username: String,
    matrix_password: String,
    strapi_user: String,
    strapi_password: String,
    admin_users: Vec<String>,
    host: SocketAddr,
    api_secret: String,
}

async fn matrix_bot(cfg: Config) -> anyhow::Result<()> {
    let client = HttpsClient::https(cfg.matrix_homeserver, None);

    // Once randomly chosen, this is now our ID.
    // Avoids creating new "devices" with every run.
    let device_id: &'static DeviceId = "TBANTADCIL".into();
    let device_name = "ferris-bot";
    client
        .log_in(
            &cfg.matrix_username,
            &cfg.matrix_password,
            Some(device_id),
            Some(device_name),
        )
        .await?;
    let bot_id = UserId::try_from(&cfg.matrix_username[..])?;

    let strapi_client = strapi::login(&cfg.strapi_user, &cfg.strapi_password).await?;

    let bot = bot::event_loop(bot_id, client.clone(), cfg.admin_users.clone(), strapi_client);

    let server = api::server(cfg.host, cfg.api_secret, cfg.admin_users, client);
    let (bot_ended, server_ended) = future::join(bot, server).await;
    bot_ended?;
    server_ended?;

    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let matrix_homeserver = env::var("MATRIX_HOMESERVER").expect("Need MATRIX_HOMESERVER");
    let matrix_homeserver = matrix_homeserver.parse()?;
    let matrix_username = env::var("MATRIX_USER").expect("Need MATRIX_USER");
    let matrix_password = env::var("MATRIX_PASSWORD").expect("Need MATRIX_PASSWORD");
    let strapi_user = env::var("STRAPI_USER").expect("Need STRAPI_USER");
    let strapi_password = env::var("STRAPI_PASSWORD").expect("Need STRAPI_PASSWORD");
    let admin_users = env::var("ADMIN_USERS").unwrap_or_else(|_| "".into());
    let admin_users = admin_users.split(",").map(|s| s.to_string()).collect();
    let host = env::var("HOST")
        .unwrap_or_else(|_| "127.0.0.1:8383".into())
        .parse()
        .expect("Invalid host");
    let api_secret = env::var("API_SECRET").expect("Need API_SECRET");

    let config = Config {
        matrix_homeserver,
        matrix_username,
        matrix_password,
        strapi_user,
        strapi_password,
        admin_users,
        host,
        api_secret,
    };

    matrix_bot(config).await
}
