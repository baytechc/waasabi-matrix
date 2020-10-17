use std::env;

use futures_util::future;
use http::Uri;
use ruma::DeviceId;
use ruma_client::HttpsClient;

mod api;
mod bot;
mod matrix;
mod strapi;

async fn matrix_bot(homeserver_url: Uri, username: &str, password: &str, strapi_user: &str, strapi_password: &str) -> anyhow::Result<()> {
    let client = HttpsClient::https(homeserver_url, None);

    // Once randomly chosen, this is now our ID.
    // Avoids creating new "devices" with every run.
    let device_id: &'static DeviceId = "TBANTADCIL".into();
    let device_name = "ferris-bot";
    client.log_in(username, password, Some(device_id), Some(device_name)).await?;

    let strapi_client = strapi::login(strapi_user, strapi_password).await?;

    let bot = bot::event_loop(client.clone(), strapi_client);
    let server = api::server(3000, client);
    let (bot_ended, server_ended) = future::join(bot, server).await;
    bot_ended?;
    server_ended?;

    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let matrix_homeserver = env::var("MATRIX_HOMESERVER").expect("Need MATRIX_HOMESERVER");
    let matrix_username = env::var("MATRIX_USER").expect("Need MATRIX_USER");
    let matrix_password = env::var("MATRIX_PASSWORD").expect("Need MATRIX_PASSWORD");
    let strapi_user = env::var("STRAPI_USER").expect("Need STRAPI_USER");
    let strapi_password = env::var("STRAPI_PASSWORD").expect("Need STRAPI_PASSWORD");

    let server = matrix_homeserver.parse()?;
    matrix_bot(server, &matrix_username, &matrix_password, &strapi_user, &strapi_password).await
}
