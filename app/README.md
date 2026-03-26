`app/` contains the gallery application example.

Main contents:
- `root/`: Rust app source, templates, public assets, migrations, `.env.example`
- `ops/alerts/`: Telegram alert helpers and watchdog examples
- `ops/backup/`: backup, restore, rehearsal, and drill scripts
- `ops/bsd/`: FreeBSD service, rebuild, rollback, and smoke-check scripts
- `ops/maintenance/`: retention and maintenance job examples

Start by copying `root/.env.example` to `.env`, then fill your own admin credentials, database URL, IndexNow key, and any Telegram alert values you want to use.
