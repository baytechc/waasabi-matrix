use std::{env, process::exit};

use futures_util::future;
use http::Uri;
use ruma::DeviceId;
use ruma_client::HttpsClient;

mod api;
mod bot;
mod matrix;

async fn matrix_bot(homeserver_url: Uri, username: &str, password: &str) -> anyhow::Result<()> {
    let client = HttpsClient::https(homeserver_url, None);

    // Once randomly chosen, this is now our ID.
    // Avoids creating new "devices" with every run.
    let device_id: &'static DeviceId = "TBANTADCIL".into();
    let device_name = "ferris-bot";
    client.log_in(username, password, Some(device_id), Some(device_name)).await?;
    let bot = bot::event_loop(client.clone());
    let server = api::server(3000, client);
    let (bot_ended, server_ended) = future::join(bot, server).await;
    bot_ended?;
    server_ended?;

    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let (homeserver_url, username, password) = match (
        env::args().nth(1),
        env::args().nth(2),
        env::var("MATRIX_PASSWORD"),
    ) {
        (Some(a), Some(b), Ok(c)) => (a, b, c),
        _ => {
            eprintln!(
                "Usage: {} <homeserver_url> <username>",
                env::args().next().unwrap()
            );
            exit(1)
        }
    };

    let server = homeserver_url.parse()?;
    matrix_bot(server, &username, &password).await
}
