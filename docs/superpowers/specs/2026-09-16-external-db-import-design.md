# External DB import — design (2026-09-16)

## Goal

Register databases Portside didn't create (pgAdmin-managed servers,
other local services, stray Docker containers) and manage them in the
app going forward: health, browse/create/drop databases, copy connect
strings. No lifecycle control over servers Portside doesn't own.

## Model (approach A: one table)

Extend `instances`:

- `origin TEXT NOT NULL DEFAULT 'managed'` — `managed` | `imported` | `adopted`
- `host TEXT NOT NULL DEFAULT '127.0.0.1'`
- `db_user TEXT NOT NULL DEFAULT ''` — empty means engine default (`root`, `postgres`)
- `ssl TEXT NOT NULL DEFAULT ''` — empty means current behavior; `off` | `require` for external rows

Migration: `ALTER TABLE` adds with defaults, so every existing row
becomes `managed` on loopback with zero behavior change.

## Backend

New commands in `src-tauri/src/` (new `import.rs` module):

- `probe_connection(engine, host, port, user, password, ssl)` — TCP +
  live query, returns ok/version/error. Powers the Test button. No writes.
- `import_external(...)` — probes first, saves `origin='imported'` only
  on success.
- `list_adoptable()` — Docker containers with DB images lacking the
  `app=portside` label: engine/tag guessed from image name, ports and
  volumes from `inspect`.
- `adopt_container(container)` — saves `origin='adopted'` with discovered
  container/volume/ports. Wipe allowed only when a volume is known.
- `list_pgadmin_servers()` — reads pgAdmin's `pgadmin4.db` at per-OS
  paths (`%APPDATA%\pgadmin\`, `~/.pgadmin/`, `~/.config/pgadmin/`).
  Returns name/host/port/username/database. Never passwords (pgAdmin
  encrypts them); the UI prompts once per entry.

Refactor `schemas.rs` / `health.rs` URL builders to take host + user
from the row instead of hardcoded loopback + engine default. Same for
`connectString` in `src/lib/tauri.ts`.

## Frontend

- Import button next to Create instance opens a dialog with three tabs:
  Connection (manual host/port/user/password/ssl + Test), Docker
  (adoptable container list), pgAdmin (prefilled server list, password
  prompt per row).
- Instance table: origin badge (Local / Imported / Adopted), endpoint
  shows the real host. Lifecycle buttons hidden for `imported`;
  `adopted` gets Start/Stop/Remove (+ Wipe when volume known).
  Danger group for `imported` is a single Forget (deletes the row,
  touches nothing else).
- Logs panel for external rows: "Logs aren't available for external
  servers." No Cert button outside LAN-managed rows.
- Passwords stored plaintext in SQLite, same as managed rows today.

## Tests

- Rust: URL builders with host/user/ssl combos; pgAdmin row parsing
  against a fixture db.
- Frontend: badge rendering, conditional buttons per origin, Forget flow.
- Manual: import a live pgAdmin server, adopt a stray container.

## Out of scope (V1)

Custom CA files for verify-ca on external servers; password rotation;
editing an imported row's connection after save (Forget + re-import).
