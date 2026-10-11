# Agent interface (CLI + MCP) — design (2026-10-11)

## Goal

Let agents (opencode, Claude Code, any MCP-capable or shell-only agent)
stand up databases dynamically in Portside from a prompt: create
versioned instance, create database/schema, get a connect string, check
health/logs, stop/wipe when done. Same-machine Docker + SQLite as the
desktop app; remote agents consume the returned LAN/TLS connect string
via the local orchestrator (no inbound daemon in V1).

## Decisions (approved)

- Local + remote agents, full lifecycle, **both CLI + MCP on a shared
  Rust core** (single binary, no logic drift).

## Architecture (approach A: shared core lib)

- Extract `src-tauri/src/{docker,state,schemas,health,catalog,certs,import}.rs`
  into pure-async/core functions with **no `#[tauri::command]`** on the
  core path. Tauri handlers become thin `#[tauri::command]` wrappers
  over `core::*` (identical behavior, identical SQLite path
  `data_dir()/portside.db`, identical `bollard` Docker socket).
- New crate member / binary `portside-cli` (clap v4, tokio): parses
  argv, calls `core`, prints human table by default and `--json` for
  agents. Single `cargo build` → `portside-cli(.exe)`.
- MCP = `portside-cli mcp` subcommand over stdio: hand-rolled
  newline-delimited JSON-RPC 2.0 on serde_json+tokio (zero new protocol
  deps, exact 11-tool surface, integration-tested). No second server
  codebase, no Node/Python runtime at invoke time. Stdio keeps it
  local-first and composable with every MCP host.

Why not B (Node MCP duplicating Docker/SQLite logic): drift risk on
every catalog/engine/TLS change. Why not C (HTTP daemon): needs auth,
firewall, always-on process; LAN connect strings already solve "remote
uses the DB" without "remote drives Docker".

## CLI surface (`portside-cli`)

```
portside-cli list [--json]
portside-cli catalog [--json]
portside-cli create --engine pg|mysql|mariadb|redis|valkey --tag 17 \
  [--port 0] [--lan] [--password P] [--wait 60] [--json]
  # --port 0 = auto via suggest_port(taken); --lan binds 0.0.0.0+TLS
  # --wait SEC polls health() until query_ok, then prints connect strings
portside-cli start|stop <id> [--json]
portside-cli rm <id> [--wipe] [--json]      # rm keeps volume; --wipe drops it
portside-cli db list <id> [--json]
portside-cli db create|drop <id> <name> [--json]
portside-cli conn <id> [--lan] [--strict] [--json]
  # prints localhost URL + (when LAN) lan URL + cert hint; pure read, no Docker calls
portside-cli logs <id> [--tail 200] [--json]
portside-cli health <id> [--wait 30] [--json]
portside-cli mcp                             # stdio MCP server (no --json needed)
```

Exit codes: 0 ok · 1 usage/validation · 2 runtime (Docker/SQLite).
`--json` prints a single JSON object to stdout; errors go as
`{ "error": "...", "hint": "is Docker running?" }` to stderr with
non-zero exit. Never log passwords beyond the returned connection
object itself.

## MCP surface (`portside-cli mcp`, stdio)

Tools (names stable V1): `list_instances, create_instance,
get_connection, create_database, list_databases, instance_health,
container_logs, start_instance, stop_instance, remove_instance,
list_catalog`. Args mirror the CLI flags (engine/tag/port=0/lan/
password/wait). Returns:

- `create_instance` → `{ id, engine, tag, port, status,
  localhost_url, lan_url|null, tls, user, password_set:bool }` plus a
  one-line human summary (host-specific secret stays in the structured
  result, not the summary).
- `get_connection` → `{ localhost_url, lan_url|null, tls, cert_pem|null-hint }`.
- Errors as MCP `Error` with the same message strings as the CLI.

`wait` semantics shared: poll `health()` (container_running && tcp_open
&& query_ok) with 1s interval up to the timeout, return the last health
on timeout as an error.

## Remote story (no daemon)

Local orchestrator (human or agent with MCP/shell) runs create, then
hands `lan_url` (+ cert via `server_cert` when strict) to the remote
agent out-of-band. Remote never talks to Docker/SQLite directly. Strict
TLS (`verify-ca`) stays opt-in; default stays pgAdmin posture
(encryption without verification).

## Files touched

- `src-tauri/`: refactor `src/*.rs` → `src/core/*.rs` + thin command
  wrappers; `Cargo.toml` (+clap dep only);
  `src/catalog.json` unchanged (shared via `include_str!`).
- New `src-cli/{main.rs, mcp.rs, output.rs}` (~600-900 LOC total).
- `docs/superpowers/specs/2026-10-11-agent-interface-design.md` (this file).
- `mcp.json` example + README/opencode config snippet
  (`"portside": { "type": "local", "command": ["…/portside-cli", "mcp"] }`).
- Release workflow: attach `portside-cli` per-OS artifact next to the
  Tauri installers (or `cargo install --path src-cli` for dev).

## Error handling

- Validation (bad engine/tag/port 0 semantics, bad password chars per
  `certs::check_password`, bad db names per `schemas::check_db_name`)
  fails fast before any Docker call.
- `create` rollbacks preserved (failed start → remove container +
  volumes, no SQLite row), surfaced identically in GUI/CLI/MCP.
- Docker-down → `hint: "is Docker running? start Docker Desktop"`.
- Timeouts (`--wait`, probe 8s) always return the partial state
  (id/port/health) so agents can retry or inspect logs.

## Testing

- Rust unit: keep existing suites green (URL builders, password policy,
  health conjunction, catalog tags); add CLI arg parsing + `--json`
  golden tests and `conn` URL parity with `src/lib/tauri.ts`.
- Integration (needs Docker): create pg17 + redis8 → wait healthy →
  create/drop db → conn strings connect → rm --wipe. Gated behind
  `PORTSIDE_E2E=1` so plain `cargo test` stays offline.
- MCP: stdio handshake smoke test (initialize → tools/list →
  tools/call `list_catalog`) against the real stdio binary.
- Manual: register `portside-cli mcp` in opencode + Claude Code, prompt
  "stand up postgres 17 with a database called demo and give me the
  connect string", verify one-shot ready URL.

## Out of scope (V1)

HTTP/SSE transport; Windows named-pipe vs TCP autodetect changes;
password rotation; editing rows post-create; custom CA upload;
multi-host orchestration; publishing the CLI to crates.io/npm.
