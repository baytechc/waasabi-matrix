use std::{convert::TryFrom, env, net::SocketAddr};

use futures_util::future;
use http::Uri;
use ruma::{DeviceId, UserId};
use ruma_client::HttpsClient;

mod api;
mod bot;
mod config;
mod dispatcher;
mod matrix;
mod strapi;

struct Config {
    matrix_homeserver: Uri,
    matrix_username: String,
    matrix_password: String,
    strapi_host: String,
    strapi_user: String,
    strapi_password: String,
    admin_users: Vec<String>,
    host: SocketAddr,
    api_secret: String,
}

async fn matrix_bot(cfg: Config) -> anyhow::Result<()> {
    let strapi_client =
        strapi::login(&cfg.strapi_host, &cfg.strapi_user, &cfg.strapi_password).await?;

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
    let cfg = args.next().expect("Missing configuration file.");
    let cfg = config::parse(&cfg).expect("Can't parse configuration file.");

    let matrix_homeserver = cfg.server.homeserver;
    let matrix_username = cfg.server.user;
    let matrix_password = cfg.server.password;
    let strapi_host = cfg.strapi.host;
    let strapi_user = cfg.strapi.user;
    let strapi_password = cfg.strapi.password;
    let admin_users = cfg.server.admins;
    let host = cfg.api.listen;
    let api_secret = cfg.api.secret;

    let config = Config {
        matrix_homeserver,
        matrix_username,
        matrix_password,
        strapi_host,
        strapi_user,
        strapi_password,
        admin_users,
        host,
        api_secret,
    };

    matrix_bot(config).await
}
