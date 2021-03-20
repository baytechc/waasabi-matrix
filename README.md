# ferris-bot - Your friendly Rusty crab, guiding you through the conference

`ferris-bot` is a Matrix chat bot that can handle logging, moderation and some admin operations.


## Configuration

The bot needs to be configured before use. The configuration file is in the [TOML](https://toml.io/en/) format. The path to the bot configuration should be specified as a commandline parameter when starting the bot.

An example configuration is provided in [`bot-config.example.toml`](./bot-config.example.toml)

The configuration consists of the following fields:


### Matrix server

The bot will connect to the Matrix server specified in the configuration and will use it to access the Matrix network.

Currently the bot [does not handle well](https://github.com/baytechc/waasabi-matrix/issues/7) being rate limited by the Matrix server so it is recommended to disable rate limiting for the bot user (and thus, we recommend using a Matrix server that allows for this). At the moment this can only be done [manually](https://github.com/matrix-org/synapse/issues/6286).

| `[matrix]`   |   |
| ------------ | - |
| `homeserver` | The URL of the Matrix server to connect to |
| `user`       | Full matrix username of the bot user |
| `password`   | Password of the bot user |
| `admins`     | A list (array) of matrix usernames who can control the bot using [bot commands](#commands) |


### Backend

The bot collects all room information and incoming messages and forwards them to the backend integration. Currently the only supported integration is [Waasabi](https://waasabi.org)'s chat integration via Strapi.

| `[backend]`  |   |
| ------------ | - |
| `host`       | URL of the exposed API root (for Waasabi servers this is by default under `<origin>/waasabi` ) |
| `user`       | Username with API access privileges (*Event Manager Integrations* role) |
| `password`   | Password of the authenticated user |


### Bot API

This bot exposes a http API that can be used to send commands to the bot through [API requests](#api).

| `[api]`      |   |
| ------------ | - |
| `listen`     | The address or ip/port combination to listen (expose the API) on |
| `secret`     | The secret that is required to be present in all API requests |


## API

### Invite a user to a room

```
POST /invite
{
    api_key: <secret string>,
    user_id: <@user:homeserver>,
    room_id: <#channel:homeserver>,
}
```

### Create a new room on the server

```
POST /room
{
    api_key: <secret string>,
    alias: <room name>,
    name: <room display name>,
    topic: <optional topic for the room>,
}
```

## Commands

These are commands that the bot understands.

| Command | Description |
| ------- | ----------- |
| `!ping` | **Admin-only**. Ping-pong with the bot. |
| `!invite <user id>` | **Admin-only**. Invite a user to the current room. |
| `!create <room alias> <room name>` | **Admin-only**. Create a new room. |
| `!op` | **Admin-only**. Give room admin access to all admin users. |
| `!op <user id>` | **Admin-only**. Add a new user to the list of admins. |
| `?ops` | **Admin-only**. List all current admin users. |

## Build

Build the code, then use the binary in `target/release/ferris-bot`:

```
cargo build --release
```

Or build install it right away:

```
cargo install --path .
```

Building this app requires `libopenssl`. On Ubuntu, use:

```
apt-get install libssl-dev
```

Otherwise you might be receiving an error message during build:
```
Package openssl was not found in the pkg-config search path.
```
