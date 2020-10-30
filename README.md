# ferris-bot - Your friendly Rusty crab, guiding you through the conference

`ferris-bot` is a Matrix chat bot that can handle logging, moderation and some admin operations.


## Configuration

`social-image` takes configuration via environment variables.

| Variable   | Deafult value  | Description |
| ---------- | -------------- | ----------- |
| API_SECRET     |                | **Required.** The authentication secret for converting SVGs. |
| HOST       | 127.0.0.1:8383 | The host and port the server listens on. |
| MATRIX_HOMESERVER | | **Required.** The homeserver to connect to. |
| MATRIX_USER | | **Required.** The full matrix user ID to use. |
| MATRIX_PASSWORD | | **Required.** the password for the matrix user. |
| STRAPI_USER | | **Required.** The user for the Strapi backend. |
| STRAPI_PASSWORD | | **Required.** The password for the Strapi backend. |
| ADMIN_USERS | *empty* | A comma-separated list of Matrix users that should have admin rights. |

## API

```
POST /invite
{
    api_key: <secret string>,
    user_id: <@user:homeserver>,
    room_id: <#channel:homeserver>,
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
