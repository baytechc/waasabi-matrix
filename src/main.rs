//! # waasabi-matrix - Your friendly Rusty crab, guiding you through the conference
//!
//! `waasabi-matrix` is a Matrix chat bot that can handle logging, moderation and some admin operations.
//! It is used for the rustfest.global conference.
//!
//! It handles a multitude of tasks:
//!
//! * Invite users to channels upon a request to the API
//!   * This lets your conference attendee management system invoke user invitations to your
//!     conference rooms.
//! * Relay messages to the backend
//!   * This way you can embed and show messages right next to your stream.
//! * Handle permissions in channels and for admin users
//!   * Privileged users can create new channels and invite users.
//! * Whatever additional command you want to implement.

use std::convert::TryFrom;
use std::env;
use std::net::SocketAddr;
use std::process;

use futures_util::future;
use http::Uri;
use ruma::{DeviceId, UserId};
use ruma_client::HttpsClient;

mod api;
mod bot;
mod config;
mod matrix;
mod strapi;

struct Config {
    matrix_homeserver: Uri,
    matrix_username: String,
    matrix_password: String,
    strapi_host: String,
    strapi_integrations_endpoint: String,
    strapi_user: String,
    strapi_password: String,
    admin_users: Vec<String>,
    host: SocketAddr,
    api_secret: String,
}

async fn matrix_bot(cfg: Config) -> anyhow::Result<()> {
    let strapi_client = strapi::login(
        &cfg.strapi_host,
        &cfg.strapi_integrations_endpoint,
        &cfg.strapi_user,
        &cfg.strapi_password,
    )
    .await?;

    let client = HttpsClient::https(cfg.matrix_homeserver, None);

    // Once randomly chosen, this is now our ID.
    // Avoids creating new "devices" with every run.
    let device_id: &'static DeviceId = "TBANTADCIL".into();
    let device_name = "waasabi-matrix";
    client
        .log_in(
            &cfg.matrix_username,
            &cfg.matrix_password,
            Some(device_id),
            Some(device_name),
        )
        .await?;
    let bot_id = UserId::try_from(&cfg.matrix_username[..])?;
    let bot = bot::event_loop(
        bot_id,
        client.clone(),
        cfg.admin_users.clone(),
        strapi_client,
    );

    let server = api::server(cfg.host, cfg.api_secret, cfg.admin_users, client);
    let (bot_ended, server_ended) = future::join(bot, server).await;
    bot_ended?;
    server_ended?;

    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let mut args = env::args().skip(1);
    let cfg = match args.next() {
        Some(arg) => arg,
        None => {
            eprintln!("Missing configuration file.");
            eprintln!();
            eprintln!("Usage: waasabi-matrix <path to config file>");
            process::exit(1);
        }
    };
    let cfg = match config::parse(&cfg) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("Can't parse configuration file.");
            eprintln!();
            eprintln!("Error: {}", e);
            process::exit(1);
        }
    };

    let matrix_homeserver = cfg.matrix.homeserver;
    let matrix_username = cfg.matrix.user;
    let matrix_password = cfg.matrix.password;
    let strapi_host = cfg.backend.host;
    let strapi_user = cfg.backend.user;
    let strapi_password = cfg.backend.password;
    let admin_users = cfg.matrix.admins;
    let host = cfg.api.listen;
    let api_secret = cfg.api.secret;

    let strapi_integrations_endpoint = cfg
        .backend
        .integrations_endpoint
        .unwrap_or("event-manager/integrations".to_string());

    let config = Config {
        matrix_homeserver,
        matrix_username,
        matrix_password,
        strapi_host,
        strapi_integrations_endpoint,
        strapi_user,
        strapi_password,
        admin_users,
        host,
        api_secret,
    };

    matrix_bot(config).await
}
