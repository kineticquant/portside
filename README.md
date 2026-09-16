# Portside

Local database launcher for devs. Spin up versioned database instances as
Docker containers, manage databases, tail logs, and copy connect strings —
from one Tauri desktop app.

A modern, open replacement for [StackBricks](https://stackbricks.app/)
(MariaDB / MySQL / PostgreSQL / Redis on your desktop), built on Tauri +
Docker with per-instance TLS for LAN sharing.

## Databases we support

| Engine | Versions (Docker tags, see `src/catalog.json`) |
|--------|-------------------------------------------------|
| MySQL | 8.4, 9.7, 26.7 |
| MariaDB | 11.4, 11.8, 12.3 |
| Postgres | 16, 17 |
| Redis | 7, 8 |
| Valkey | 8 |

Multiple versions of the same engine run side by side on different ports.

## Run it

Needs Docker running.

```bash
npm install
npm run tauri dev
```

Windows one-click dev flow: double-click `run.bat` (checks the toolchain,
starts Docker, launches the app). Details in `TOOLCHAIN.md`.

## How you use it

- **Create instance** — pick engine, version, port (blank = auto), and an
  optional password. Defaults: `portside` (SQL), empty (redis/valkey).
  The password is set once and can't be changed later.
- **Copy connect** — localhost string. **Copy LAN** — laptop-facing string
  using this machine's LAN IP (LAN instances only).
- **LAN (TLS)** — lets other machines on your network connect. The instance
  encrypts with a self-signed cert; clients don't verify it by default.
- **Lifecycle** — Inactivate stops the container but keeps data. Remove
  drops the container but keeps the data volume. Wipe drops both (no
  restore, confirms first).

Instance metadata lives in SQLite (`%LOCALAPPDATA%\portside\portside.db`);
actual data lives in Docker volumes (`portside-{id}-data`).

## Tests

```bash
npx vitest run                                    # frontend
cargo test --manifest-path src-tauri/Cargo.toml   # Rust (needs cargo)
```

Releases are built by `.github/workflows/release.yml` for Windows, macOS
(Intel + ARM), and Linux.
