# Gallery Web App

<img width="966" height="877" alt="1" src="https://github.com/user-attachments/assets/355736b1-3cbd-4b99-bb00-8887c4fb3363" />
<br>
This is a gallery website I built and decided to share as a public example.

## Layout

- `web/`: reverse proxy and watchdog examples for the public-facing host
- `app/`: Rust application source, templates, static files, database migrations, and host scripts

## What Each Server Needs

This example assumes two FreeBSD servers:

- `web`
- `app`

### Web server

- FreeBSD
- `nginx`
- a public domain
- TLS certificates for that domain
- network access to the app server on port `8080`
- optional Telegram bot token and chat id if you want alerting

### App server

- FreeBSD
- Rust toolchain with `cargo`
- a C toolchain such as `gcc` or `clang`
- `pkg-config`
- PostgreSQL
- writable directories for:
  - `uploads/`
  - `uploads/thumbs/`
  - `run/`

## Notes About Other Operating Systems

You may be able to run this on another operating system if you can install equivalent packages there.

If you do that, you are responsible for adapting the differences yourself, including:

- service management
- package names
- filesystem paths
- OS-specific setup details

## App Server Setup

1. Copy `app/root/.env.example` to `app/root/.env`.
2. Fill in your own admin usernames, password hashes, API key hashes, database URL, and IndexNow key.
3. Create a PostgreSQL database.
4. Start the app from `app/root/`.

Example:

```bash
cd app/root
cargo run --release
```

The app listens on `0.0.0.0:8080` by default.
You can override that with `BIND_ADDR` and `PORT`.

## Web Server Setup

1. Open `web/nginx/nginx.conf`.
2. Replace all `<...>` placeholders with your own values.
3. Point the upstream to your app server address.
4. Install the config into nginx.
5. Reload or restart nginx.

## Alerts

The `web/alerts/` and `app/ops/alerts/` directories contain Telegram alert examples.
Replace the `<...>` placeholders before using them.

## Important Files

- `app/root/.env.example`
- `app/root/Cargo.toml`
- `app/root/src/main.rs`
- `app/root/migrations/`
- `web/nginx/nginx.conf`
- `web/alerts/`

## Screenshots

<img width="966" height="877" alt="1" src="https://github.com/user-attachments/assets/355736b1-3cbd-4b99-bb00-8887c4fb3363" />
<img width="966" height="877" alt="2" src="https://github.com/user-attachments/assets/aa2cc51c-2a60-43b9-bd98-a63851cd68fa" />
<img width="966" height="877" alt="3" src="https://github.com/user-attachments/assets/dcd09345-ebb1-480c-a98d-8208f2dbcae8" />
<img width="966" height="10647" alt="4" src="https://github.com/user-attachments/assets/183ccdeb-5cbb-4018-b8f1-121be9f2f201" />

## License

MIT, by itinfra7 from GitHub.
