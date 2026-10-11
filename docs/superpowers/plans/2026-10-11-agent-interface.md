# Agent Interface (CLI + MCP) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give agents a headless way to drive Portside: a `portside-cli` binary (human + `--json` output) and a `portside-cli mcp` stdio MCP server, both calling the exact same Rust core as the desktop app.

**Architecture:** Add `src-tauri/src/lib.rs` exposing the existing modules as a library (no file moves, no function-signature changes — `#[tauri::command]` fns are directly callable); rewrite `src-tauri/src/main.rs` as a thin Tauri wrapper over the lib; add `src-tauri/src/bin/portside-cli/` (clap CLI + hand-rolled newline-delimited JSON-RPC MCP server using the already-vendored `serde_json`/`tokio`, no new protocol crates).

**Tech Stack:** Rust 2021, tokio (present), serde/serde_json (present), bollard/rusqlite/sqlx/redis/rcgen/uuid/dirs (present), clap 4 derive (new, only addition).

**Spec:** `docs/superpowers/specs/2026-10-11-agent-interface-design.md`

## Global Constraints

- Same SQLite path (`data_dir()/portside.db`) and same `bollard` Docker socket as the GUI — CLI/MCP/GUI see identical state; no new state locations.
- Password policy (`certs::check_password`) and db-name policy (`check_db_name` semantics: `[a-zA-Z0-9_]+`) enforced before any Docker call.
- `--json` prints ONE JSON object to stdout; errors print `{ "error": "...", "hint": "..." }` to stderr; exit 0 ok · 1 usage/validation · 2 runtime.
- Never log passwords; MCP summaries stay secret-free (secrets only in structured results).
- Test ports must avoid Hyper-V/WSL excluded ranges — reuse e2e constants (PG 55433, REDIS 56380, MYSQL 13307).
- Rust ≥ 1.77, Node ≥ 20 for frontend checks; `cargo test` stays offline-capable (Docker tests keep `#[ignore]`).

---

### Task 1: Core library layout (zero behavior change)

**Files:**
- Create: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/main.rs`
- Modify: `src-tauri/Cargo.toml` (add clap only; no other deps)

**Interfaces:**
- Consumes: existing modules `catalog, certs, docker, health, import, prereqs, schemas, state` as-is.
- Produces: library crate `portside` exposing `pub mod catalog, certs, docker, health, import, prereqs, schemas, state`; bin targets link it as `portside::`.

- [ ] **Step 1: Create `src-tauri/src/lib.rs`**

```rust
//! Shared core: the desktop app, the CLI, and the MCP server all call this.
//! Tauri `#[command]` functions are plain async fns — directly reusable.
pub mod catalog;
pub mod certs;
pub mod docker;
#[cfg(test)]
mod e2e;
pub mod health;
pub mod import;
pub mod prereqs;
pub mod schemas;
pub mod state;
```

- [ ] **Step 2: Rewrite `src-tauri/src/main.rs` as a thin wrapper (same handler list, same behavior)**

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use portside::{catalog, certs, docker, health, import, prereqs, schemas};

#[tauri::command]
fn ping() -> String {
    "pong".to_string()
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            ping,
            docker::detect_runtime,
            docker::create_instance,
            docker::start_instance,
            docker::stop_instance,
            docker::remove_instance,
            docker::list_instances,
            catalog::get_catalog,
            catalog::suggest_port,
            certs::lan_addr,
            certs::server_cert,
            schemas::list_databases,
            schemas::create_database,
            schemas::drop_database,
            schemas::list_schemas,
            schemas::redis_info,
            health::container_logs,
            health::health,
            import::probe_connection,
            import::import_external,
            import::list_adoptable,
            import::adopt_container,
            import::list_pgadmin_servers,
            import::forget_instance,
            prereqs::check_prereqs,
            prereqs::install_wsl,
            prereqs::start_docker,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run portside");
}
```

Note: `mod ...;` declarations are REMOVED from main.rs (they now live in lib.rs). `ping` stays local. Nothing else in any module file changes in this task.

- [ ] **Step 3: Add clap to `src-tauri/Cargo.toml`**

```toml
clap = { version = "4", features = ["derive"] }
```

- [ ] **Step 4: Verify lib + bins resolve and all existing tests pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: all suites PASS (docker/schemas/health/certs/catalog/import unit tests; e2e `ignored`).

- [ ] **Step 5: Verify frontend untouched**

Run: `npx vitest run`
Expected: PASS. (Workdir: repo root.)

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/lib.rs src-tauri/src/main.rs src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "refactor: expose portside core as lib for CLI/MCP reuse"
```

Fallback (only if `cargo check` shows tauri-cli confusion from multiple targets): set `autobins = false` in `src-tauri/Cargo.toml` with explicit `[[bin]] name = "portside" path = "src/main.rs"` — do NOT move the CLI to another crate without asking.

---

### Task 2: User-facing connection URLs + wait helper (new core API)

**Files:**
- Create: `src-tauri/src/connect.rs`
- Modify: `src-tauri/src/lib.rs` (add `pub mod connect;`)
- Modify: `src-tauri/src/health.rs` (append `wait_healthy`)

**Interfaces:**
- Consumes: `docker::Instance` (fields `engine, port, host, db_user, password, bind_ip, origin, ssl`; methods `db_user_or_default()`, `tls()`), `certs::lan_ip()`.
- Produces: `connect::ConnectionInfo { localhost_url: String, lan_url: Option<String>, tls: bool }`, `connect::for_instance(inst: &Instance, lan_ip: Option<&str>, strict: bool) -> ConnectionInfo`; `health::wait_healthy(id: &str, timeout_secs: u64) -> Result<Health, String>`.

- [ ] **Step 1: Write failing test for URL parity with `src/lib/tauri.ts` (postgres example)**

Append to new file `src-tauri/src/connect.rs` (skeleton + test first):

```rust
use crate::docker::Instance;

#[derive(Debug, Clone, serde::Serialize)]
pub struct ConnectionInfo {
    pub localhost_url: String,
    pub lan_url: Option<String>,
    pub tls: bool,
}

pub fn for_instance(_inst: &Instance, _lan_ip: Option<&str>, _strict: bool) -> ConnectionInfo {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pg_localhost_url_matches_frontend() {
        let inst = Instance::new("abc123", "postgres", "17", 5433, "127.0.0.1", "portside");
        let c = for_instance(&inst, None, false);
        assert_eq!(c.localhost_url, "postgres://postgres:portside@127.0.0.1:5433/postgres");
        assert_eq!(c.lan_url, None);
        assert!(!c.tls);
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml connect::`
Expected: FAIL with `not yet implemented`.

- [ ] **Step 3: Implement `for_instance` (mirror of `connectString` in `src/lib/tauri.ts`)**

```rust
fn default_password(engine: &str) -> String {
    if engine == "redis" || engine == "valkey" {
        String::new()
    } else {
        "portside".to_string()
    }
}

fn user_for(engine: &str) -> &str {
    if engine == "postgres" { "postgres" } else { "root" }
}

fn build_url(inst: &Instance, host: &str, strict: bool) -> String {
    let pw = if inst.password.is_empty() {
        default_password(&inst.engine)
    } else {
        inst.password.clone()
    };
    let user = if inst.db_user.is_empty() {
        user_for(&inst.engine).to_string()
    } else {
        inst.db_user.clone()
    };
    let tls = inst.tls();
    match inst.engine.as_str() {
        "postgres" => {
            let userinfo = if pw.is_empty() { format!("{user}@") } else { format!("{user}:{pw}@") };
            let ssl = if tls {
                if strict { "?sslmode=verify-ca" } else { "?sslmode=require" }
            } else { "" };
            format!("postgres://{userinfo}{host}:{}/{ssl}", inst.port, "")
                .replace("{ssl}", "")
                .replace('?', if ssl.is_empty() { "" } else { "?" })
                .trim_end_matches('/')
                .to_string()
        }
        "redis" | "valkey" => {
            let scheme = if tls { "rediss" } else { "redis" };
            let auth = if pw.is_empty() { String::new() } else { format!(":{pw}@") };
            format!("{scheme}://{auth}{host}:{}/0", inst.port)
        }
        _ => {
            let userinfo = if pw.is_empty() { format!("{user}@") } else { format!("{user}:{pw}@") };
            let ssl = if tls {
                if strict { "?ssl-mode=VERIFY_CA" } else { "?ssl-mode=REQUIRED" }
            } else { "" };
            format!("mysql://{userinfo}{host}:{}{}", inst.port, "/mysql")
                + ssl
        }
    }
}

pub fn for_instance(inst: &Instance, lan_ip: Option<&str>, strict: bool) -> ConnectionInfo {
    let tls = inst.tls();
    ConnectionInfo {
        localhost_url: build_url(inst, "127.0.0.1", strict),
        lan_url: lan_ip.map(|ip| build_url(inst, ip, strict)),
        tls,
    }
}
```

NOTE: the postgres arm above is deliberately clumsy — rewrite it cleanly as:

```rust
"postgres" => {
    let userinfo = if pw.is_empty() { format!("{user}@") } else { format!("{user}:{pw}@") };
    let ssl = if tls {
        if strict { "?sslmode=verify-ca" } else { "?sslmode=require" }
    } else {
        ""
    };
    format!("postgres://{userinfo}{host}:{}/postgres{ssl}", inst.port)
}
```

and the mysql arm as:

```rust
_ => {
    let userinfo = if pw.is_empty() { format!("{user}@") } else { format!("{user}:{pw}@") };
    let ssl = if tls {
        if strict { "?ssl-mode=VERIFY_CA" } else { "?ssl-mode=REQUIRED" }
    } else {
        ""
    };
    format!("mysql://{userinfo}{host}:{}/mysql{ssl}", inst.port)
}
```

- [ ] **Step 4: Add the remaining parity tests (mysql, redis, TLS lan, strict)**

```rust
#[test]
fn mysql_localhost_url_matches_frontend() {
    let inst = Instance::new("a", "mysql", "8.4", 3307, "127.0.0.1", "portside");
    let c = for_instance(&inst, None, false);
    assert_eq!(c.localhost_url, "mysql://root:portside@127.0.0.1:3307/mysql");
}

#[test]
fn redis_url_has_db_zero_and_no_auth_by_default() {
    let inst = Instance::new("a", "redis", "8", 6380, "127.0.0.1", "");
    let c = for_instance(&inst, None, false);
    assert_eq!(c.localhost_url, "redis://127.0.0.1:6380/0");
}

#[test]
fn lan_instance_reports_tls_and_lan_url() {
    let inst = Instance::new("a", "postgres", "17", 5433, "0.0.0.0", "s3cret!");
    let c = for_instance(&inst, Some("192.168.1.10"), false);
    assert!(c.tls);
    assert_eq!(
        c.lan_url.as_deref().unwrap(),
        "postgres://postgres:s3cret!@192.168.1.10:5433/postgres?sslmode=require"
    );
}

#[test]
fn strict_tls_uses_verify_ca() {
    let inst = Instance::new("a", "postgres", "17", 5433, "0.0.0.0", "s3cret!");
    let c = for_instance(&inst, Some("192.168.1.10"), true);
    assert!(c.lan_url.as_deref().unwrap().ends_with("?sslmode=verify-ca"));
    let local = for_instance(
        &Instance::new("b", "mysql", "8.4", 3307, "127.0.0.1", "portside"),
        None,
        true,
    );
    assert_eq!(local.localhost_url, "mysql://root:portside@127.0.0.1:3307/mysql");
}
```

- [ ] **Step 5: Append `wait_healthy` to `src-tauri/src/health.rs`**

```rust
/// Poll `health(id)` once per second until healthy or timeout.
/// Returns the last Health on timeout as an Err with a one-line summary.
pub async fn wait_healthy(id: &str, timeout_secs: u64) -> Result<Health, String> {
    let mut last = health(id.to_string()).await?;
    for _ in 0..timeout_secs.max(1) {
        if last.is_healthy() {
            return Ok(last);
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        last = health(id.to_string()).await?;
    }
    if last.is_healthy() {
        Ok(last)
    } else {
        Err(format!(
            "instance {id} not healthy after {timeout_secs}s \
             (container_running={} tcp_open={} query_ok={})",
            last.container_running, last.tcp_open, last.query_ok
        ))
    }
}
```

`Health` needs `Copy` — derive is `Debug, Clone, Copy, PartialEq, Eq, Serialize` already. No change.

- [ ] **Step 6: Register module + run tests**

In `src-tauri/src/lib.rs` add `pub mod connect;` (alphabetical: after `certs`, before `docker`).

Run: `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: PASS including 5 new `connect::` tests.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/connect.rs src-tauri/src/lib.rs src-tauri/src/health.rs
git commit -m "feat(core): user-facing connection URLs + wait_healthy"
```

---

### Task 3: CLI skeleton — clap wiring + read-only commands

**Files:**
- Create: `src-tauri/src/bin/portside-cli/main.rs` (clap defs + dispatch)
- Create: `src-tauri/src/bin/portside-cli/cmds.rs` (handlers)
- Create: `src-tauri/src/bin/portside-cli/output.rs` (human vs `--json` rendering)

**Interfaces:**
- Consumes: `portside::{catalog, certs, connect, docker, health, schemas, state}`; Task 2 API.
- Produces: binary `portside-cli` with `list, catalog, conn, health, logs, db list`; `output::print_json(&impl Serialize)` + `output::fail(msg, hint) -> !` (stderr JSON + exit 2).

- [ ] **Step 1: Write `output.rs`**

```rust
use serde::Serialize;

pub fn print_json<T: Serialize>(v: &T) {
    println!("{}", serde_json::to_string_pretty(v).unwrap_or_else(|_| "{}".to_string()));
}

/// CLI usage/validation failure: exit 1, plain message on stderr.
pub fn usage(msg: &str) -> ! {
    eprintln!("error: {msg}");
    std::process::exit(1);
}

/// Runtime failure: JSON error object on stderr, exit 2.
pub fn fail(err: String, hint: &str) -> ! {
    let v = serde_json::json!({ "error": err, "hint": hint });
    eprintln!("{v}");
    std::process::exit(2);
}

pub fn docker_hint() -> &'static str {
    "is Docker running? start Docker Desktop and retry"
}
```

- [ ] **Step 2: Write clap defs in `main.rs` (full command tree V1)**

```rust
mod cmds;
mod output;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "portside-cli", version, about = "Headless control for Portside database instances")]
struct Cli {
    /// Machine-readable JSON output (for agents and scripts)
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// List managed instances
    List,
    /// Show the bundled engine catalog
    Catalog,
    /// Start / stop an instance
    Start { id: String },
    Stop { id: String },
    /// Remove container (keep volume) or --wipe (drop volume too)
    Rm { id: String, #[arg(long)] wipe: bool },
    /// Create an instance; --port 0 = auto; waits until healthy then prints connect strings
    Create {
        #[arg(long)] engine: String,
        #[arg(long)] tag: String,
        #[arg(long, default_value_t = 0)] port: u16,
        #[arg(long)] lan: bool,
        #[arg(long, default_value = "")] password: String,
        #[arg(long, default_value_t = 60)] wait: u64,
    },
    /// Databases inside an instance
    Db {
        #[command(subcommand)]
        cmd: DbCmd,
    },
    /// Print connect strings (localhost always; LAN when --lan and a LAN IP is detectable)
    Conn {
        id: String,
        #[arg(long)] lan: bool,
        #[arg(long)] strict: bool,
    },
    /// Tail container logs
    Logs {
        id: String,
        #[arg(long, default_value_t = 200)] tail: usize,
    },
    /// Health check (single shot, or --wait SECONDS polling)
    Health {
        id: String,
        #[arg(long, default_value_t = 0)] wait: u64,
    },
    /// MCP server over stdio (for agents)
    Mcp,
}

#[derive(Subcommand)]
enum DbCmd {
    List { id: String },
    Create { id: String, name: String },
    Drop { id: String, name: String },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    cmds::run(cli.cmd, cli.json).await;
}
```

`cmds::run` takes `Cmd` — but `Cmd` is defined in main.rs. Fix: define `Cli`/`Cmd`/`DbCmd` in `cmds.rs` (or a `cli.rs`) and `use` from main.rs. Concretely: put the structs/enums in `cmds.rs` as `pub`, main.rs does `mod cmds; mod output; use cmds::{Cli};` with `#[derive(Parser)]` visible via `clap::Parser` import in cmds.rs. Adjust: main.rs =

```rust
mod cmds;
mod mcp;
mod output;

use clap::Parser;
use cmds::Cli;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    if matches!(cli.cmd, cmds::Cmd::Mcp) {
        mcp::serve().await;
        return;
    }
    cmds::run(cli.cmd, cli.json).await;
}
```

(`mod mcp;` references Task 5's file — add `#[cfg]`? No: create the file in this task as a stub? Better: add `mod mcp;` only in Task 5. In THIS task main.rs omits the mcp lines and `Mcp` handling is a `usage("mcp server lands in Task 5")` arm. Write it that way now; Task 5 edits main.rs to wire `mcp::serve()`.)

- [ ] **Step 3: Implement read-only handlers in `cmds.rs`**

```rust
use crate::output::{docker_hint, fail, print_json, usage};
use clap::{Parser, Subcommand};
use serde::Serialize;

#[derive(Parser)]
#[command(name = "portside-cli", version, about = "Headless control for Portside database instances")]
pub struct Cli {
    #[arg(long, global = true)]
    pub json: bool,
    #[command(subcommand)]
    pub cmd: Cmd,
}

// ... Cmd / DbCmd enums exactly as in Step 2 (pub variants) ...

pub async fn run(cmd: Cmd, as_json: bool) {
    match cmd {
        Cmd::List => {
            let instances = portside::docker::list_instances()
                .await
                .unwrap_or_else(|e| fail(e, docker_hint()));
            if as_json {
                print_json(&instances);
            } else if instances.is_empty() {
                println!("no instances");
            } else {
                for i in &instances {
                    println!("{}  {}:{}  {}  {}", i.id, i.engine, i.port, i.status, i.container);
                }
            }
        }
        Cmd::Catalog => {
            let cat = portside::catalog::bundled_catalog();
            if as_json {
                print_json(&cat);
            } else {
                for e in &cat.entries {
                    println!("{}:{}  ({})", e.engine, e.tag, e.image);
                }
            }
        }
        Cmd::Conn { id, lan, strict } => {
            let inst = portside::state::load_instance(&id)
                .unwrap_or_else(|e| usage(&e));
            let lan_ip = if lan { portside::certs::lan_ip() } else { None };
            if lan && lan_ip.is_none() {
                usage("no LAN address detected (offline?) — retry without --lan");
            }
            let info = portside::connect::for_instance(&inst, lan_ip.as_deref(), strict);
            if as_json {
                print_json(&info);
            } else {
                println!("{}", info.localhost_url);
                if let Some(url) = info.lan_url {
                    println!("{url}");
                }
            }
        }
        Cmd::Health { id, wait } => {
            let h = if wait > 0 {
                portside::health::wait_healthy(&id, wait)
                    .await
                    .unwrap_or_else(|e| fail(e, docker_hint()))
            } else {
                portside::health::health(id.clone())
                    .await
                    .unwrap_or_else(|e| fail(e, docker_hint()))
            };
            if as_json {
                print_json(&h);
            } else {
                println!(
                    "{}  container={} tcp={} query={}",
                    if h.is_healthy() { "healthy" } else { "degraded" },
                    h.container_running, h.tcp_open, h.query_ok
                );
            }
        }
        Cmd::Logs { id, tail } => {
            let logs = portside::health::container_logs(id, Some(tail))
                .await
                .unwrap_or_else(|e| fail(e, docker_hint()));
            if as_json {
                print_json(&serde_json::json!({ "logs": logs }));
            } else {
                print!("{logs}");
            }
        }
        Cmd::Db { cmd } => match cmd {
            DbCmd::List { id } => {
                let dbs = portside::schemas::list_databases(id)
                    .await
                    .unwrap_or_else(|e| fail(e, docker_hint()));
                if as_json { print_json(&dbs); }
                else if dbs.is_empty() { println!("no databases"); }
                else { for d in &dbs { println!("{d}"); } }
            }
            _ => usage("db create/drop land in Task 4"),
        },
        _ => usage("lifecycle commands land in Task 4"),
    }
}
```

`Health::is_healthy` is defined on the struct — accessible. `bundled_catalog()` is `pub`. `lan_ip()` is `pub`. `load_instance` is `pub`. All confirmed in source.

- [ ] **Step 4: Build the binary**

Run: `cargo build --manifest-path src-tauri/Cargo.toml --bin portside-cli`
Expected: builds clean (warnings ok, errors not).

- [ ] **Step 5: Smoke-test read-only commands offline**

Run: `.\src-tauri\target\debug\portside-cli.exe catalog --json` (Windows) or `./src-tauri/target/debug/portside-cli catalog --json`
Expected: JSON with `entries` including `postgres:17`; exit 0.

Run: `portside-cli list --json` with Docker stopped
Expected: `[]` (list degrades to saved rows when Docker is unreachable — see `list_instances` early return) OR JSON error with hint; either way exit 0/2, no panic.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/bin/portside-cli/main.rs src-tauri/src/bin/portside-cli/cmds.rs src-tauri/src/bin/portside-cli/output.rs
git commit -m "feat(cli): read-only commands (list/catalog/conn/health/logs/db-list)"
```

---

### Task 4: CLI lifecycle — create/start/stop/rm + db create/drop

**Files:**
- Modify: `src-tauri/src/bin/portside-cli/cmds.rs`

**Interfaces:**
- Consumes: `portside::{catalog, docker, schemas, state, health, connect, certs}` + Task 3 `output` helpers.
- Produces: full CLI surface except `mcp`.

- [ ] **Step 1: Replace the `_ => usage(...)` arm with Start/Stop/Rm**

```rust
Cmd::Start { id } => {
    portside::docker::start_instance(id)
        .await
        .unwrap_or_else(|e| fail(e, docker_hint()));
    if as_json { print_json(&serde_json::json!({ "ok": true })); }
    else { println!("started"); }
}
Cmd::Stop { id } => {
    portside::docker::stop_instance(id)
        .await
        .unwrap_or_else(|e| fail(e, docker_hint()));
    if as_json { print_json(&serde_json::json!({ "ok": true })); }
    else { println!("stopped"); }
}
Cmd::Rm { id, wipe } => {
    portside::docker::remove_instance(id, wipe)
        .await
        .unwrap_or_else(|e| fail(e, docker_hint()));
    if as_json { print_json(&serde_json::json!({ "ok": true, "wipe": wipe })); }
    else if wipe { println!("removed and wiped"); }
    else { println!("removed (volume kept)"); }
}
```

- [ ] **Step 2: Implement Create (port 0 = auto, tag default, --wait, connect output)**

```rust
Cmd::Create { engine, tag, port, lan, password, wait } => {
    let tag = if tag.is_empty() {
        let cat = portside::catalog::bundled_catalog();
        cat.entries.iter().find(|e| e.engine == engine).map(|e| e.tag.clone())
            .unwrap_or_else(|| usage(&format!("unknown engine {engine}")))
    } else { tag };
    let port = if port == 0 {
        let taken: Vec<u16> = portside::state::list_saved_instances()
            .map(|v| v.iter().map(|i| i.port).collect())
            .unwrap_or_default();
        portside::catalog::suggest_port(engine.clone(), taken)
            .unwrap_or_else(|e| usage(&e))
    } else { port };
    let bind_ip = if lan { "0.0.0.0" } else { "127.0.0.1" };
    let inst = portside::docker::create_instance(
        engine, tag, port, Some(bind_ip.to_string()), Some(password),
    ).await.unwrap_or_else(|e| fail(e, docker_hint()));
    if wait > 0 {
        if let Err(e) = portside::health::wait_healthy(&inst.id, wait).await {
            fail(e, "instance created but unhealthy — check `portside-cli logs <id>`");
        }
    }
    let fresh = portside::state::load_instance(&inst.id).unwrap_or(inst);
    let lan_ip = if lan { portside::certs::lan_ip() } else { None };
    let info = portside::connect::for_instance(&fresh, lan_ip.as_deref(), false);
    if as_json {
        print_json(&serde_json::json!({
            "id": fresh.id, "engine": fresh.engine, "tag": fresh.tag,
            "port": fresh.port, "status": fresh.status,
            "localhost_url": info.localhost_url, "lan_url": info.lan_url,
            "tls": info.tls,
        }));
    } else {
        println!("created {} ({}:{} {})", fresh.id, fresh.engine, fresh.port, fresh.status);
        println!("{}", info.localhost_url);
        if let Some(url) = info.lan_url { println!("{url}"); }
    }
}
```

Note: `suggest_port(engine: String, taken: Vec<u16>)` and `create_instance(engine, tag, port, bind_ip: Option<String>, password: Option<String>)` signatures confirmed in source. `list_saved_instances()` returns `Result<Vec<Instance>, String>` — `.map(...).unwrap_or_default()` works since `Vec<u16>: Default`.

- [ ] **Step 3: Implement Db create/drop (replace the Task 3 placeholder arm)**

```rust
DbCmd::Create { id, name } => {
    portside::schemas::create_database(id, name)
        .await
        .unwrap_or_else(|e| {
            // Validation errors (bad name) are usage errors, not runtime.
            if e.contains("must be") || e.contains("SQL-only") { usage(&e); }
            fail(e, docker_hint())
        });
    if as_json { print_json(&serde_json::json!({ "ok": true })); }
    else { println!("created"); }
}
DbCmd::Drop { id, name } => { /* same shape, calling drop_database */ }
```

- [ ] **Step 4: Unit test — auto-port and tag-default logic without Docker**

The port/tag resolution is inline in the match arm (untestable). Extract it:

```rust
pub fn resolve_create(engine: &str, tag: &str, port: u16) -> Result<(String, u16), String> {
    let tag = if tag.is_empty() {
        portside::catalog::bundled_catalog().entries.iter()
            .find(|e| e.engine == engine).map(|e| e.tag.clone())
            .ok_or_else(|| format!("unknown engine {engine}"))?
    } else { tag.to_string() };
    let port = if port == 0 {
        let taken: Vec<u16> = portside::state::list_saved_instances()
            .map(|v| v.iter().map(|i| i.port).collect())
            .unwrap_or_default();
        portside::catalog::suggest_port(engine.to_string(), taken)?
    } else { port };
    Ok((tag, port))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resolve_defaults_tag_and_port() {
        let (tag, port) = resolve_create("postgres", "", 0).expect("resolves");
        assert_eq!(tag, "16"); // first postgres entry in src/catalog.json
        assert_eq!(port, 5432); // base port when nothing is taken
    }
    #[test]
    fn resolve_rejects_unknown_engine() {
        assert!(resolve_create("oracle", "", 0).is_err());
    }
    #[test]
    fn resolve_keeps_explicit_values() {
        let (tag, port) = resolve_create("redis", "8", 56380).unwrap();
        assert_eq!((tag.as_str(), port), ("8", 56380));
    }
}
```

And the Create arm calls `resolve_create(&engine, &tag, port)` then proceeds. (Fold this refactor into the same edit as Step 2.)

- [ ] **Step 5: Run tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --bin portside-cli`
Expected: 3 new tests PASS.

Run: `cargo test --manifest-path src-tauri/Cargo.toml` (full suite)
Expected: PASS, no regressions.

- [ ] **Step 6: Docker smoke test (only when Docker is running; skip otherwise)**

```bash
portside-cli create --engine redis --tag 8 --port 56380 --wait 60 --json
portside-cli db list <id-from-above> --json
portside-cli rm <id-from-above> --wipe --json
```

Expected: create prints `rediss?/redis://127.0.0.1:56380/0` with healthy status; db list prints `["default (... keys)"]`; rm prints ok. (Redis pulls fast; postgres equivalent uses port 55433 + `--tag 17`.)

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/bin/portside-cli/cmds.rs
git commit -m "feat(cli): lifecycle commands (create/start/stop/rm/db)"
```

---

### Task 5: MCP stdio server (`portside-cli mcp`) + integration test

**Files:**
- Create: `src-tauri/src/bin/portside-cli/mcp.rs`
- Create: `src-tauri/tests/mcp_stdio.rs`
- Modify: `src-tauri/src/bin/portside-cli/main.rs` (wire `Mcp` arm to `mcp::serve()`)
- Modify: `docs/superpowers/specs/2026-10-11-agent-interface-design.md` (replace `rmcp` with hand-rolled JSON-RPC + rationale: zero new deps, exact protocol surface, fully testable)

**Interfaces:**
- Consumes: `portside::{catalog, certs, connect, docker, health, schemas, state}`; `cmds::resolve_create`.
- Produces: `mcp::serve()` — newline-delimited JSON-RPC 2.0 over stdin/stdout; 10 tools.

Protocol contract (MCP stdio, JSON-RPC 2.0, one message per line):
- `initialize` (any params) → `{ protocolVersion: "2024-11-05", capabilities: { tools: {} }, serverInfo: { name: "portside", version: CARGO_PKG_VERSION } }`.
- `notifications/*` (no id) → no response.
- `tools/list` → `{ tools: [ { name, description, inputSchema } x10 ] }`.
- `tools/call` `{ name, arguments }` → `{ content: [{ type: "text", text }], isError: bool }`. Runtime failures return `isError: true` (NOT protocol errors). Unknown tool → protocol error `-32602`.
- Unknown method (with id) → `-32601`. Unparseable line → `{ id: null, error: -32700 }`.
- ALL logging to stderr. stdout is protocol only.

- [ ] **Step 1: Write `mcp.rs` — frame loop + initialize + tools/list**

```rust
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Deserialize)]
struct Req {
    #[allow(dead_code)]
    jsonrpc: String,
    #[serde(default)]
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

fn ok(id: &Value, result: Value) -> String {
    serde_json::to_string(&json!({ "jsonrpc": "2.0", "id": id, "result": result })).unwrap()
}

fn err(id: &Value, code: i32, message: &str) -> String {
    serde_json::to_string(&json!({ "jsonrpc": "2.0", "id": id,
        "error": { "code": code, "message": message } })).unwrap()
}

fn tool(name: &str, desc: &str, schema: Value) -> Value {
    json!({ "name": name, "description": desc, "inputSchema": schema })
}

fn obj(props: &[(&str, &str)], required: &[&str]) -> Value {
    let properties: serde_json::Map<String, Value> = props.iter()
        .map(|(k, t)| (k.to_string(), json!({ "type": t }))).collect();
    json!({ "type": "object", "properties": properties, "required": required })
}

fn tools_list() -> Value {
    json!({ "tools": [
        tool("list_instances", "List Portside database instances", obj(&[], &[])),
        tool("list_catalog", "Show available engine versions", obj(&[], &[])),
        tool("create_instance", "Create a database instance; port 0 = auto; waits until healthy and returns connect URLs",
            obj(&[("engine","string"),("tag","string"),("port","integer"),("lan","boolean"),("password","string"),("wait","integer")], &["engine"])),
        tool("get_connection", "Connect strings for an instance (localhost + LAN)",
            obj(&[("id","string"),("lan","boolean"),("strict","boolean")], &["id"])),
        tool("create_database", "CREATE DATABASE (SQL engines)", obj(&[("id","string"),("name","string")], &["id","name"])),
        tool("list_databases", "List databases in an instance", obj(&[("id","string")], &["id"])),
        tool("instance_health", "Container/tcp/query health; optional wait seconds",
            obj(&[("id","string"),("wait","integer")], &["id"])),
        tool("container_logs", "Tail container logs", obj(&[("id","string"),("tail","integer")], &["id"])),
        tool("start_instance", "Start a stopped instance", obj(&[("id","string")], &["id"])),
        tool("stop_instance", "Stop a running instance", obj(&[("id","string")], &["id"])),
        // NOTE: 11 entries — remove_instance intentionally separate below.
    ]})
}
```

Wait — spec says 10 tools INCLUDING remove_instance. Fix the list: replace `stop_instance` trailing comma structure so the final array is exactly:
`list_instances, create_instance, get_connection, create_database, list_databases, instance_health, container_logs, start_instance, stop_instance, remove_instance, list_catalog` = 11 names. Spec text says "10 tools" but enumerates 11 (count them: list_instances(1) create_instance(2) get_connection(3) create_database(4) list_databases(5) instance_health(6) container_logs(7) start_instance(8) stop_instance(9) remove_instance(10) list_catalog(11)). So the array has 11 entries; spec's "10" is a counting slip. Add the missing one:

```rust
tool("remove_instance", "Remove container; wipe=true also drops the data volume (irreversible)",
    obj(&[("id","string"),("wipe","boolean")], &["id"])),
```

and fix the spec's "10 tools" → "11 tools" in the doc-amendment step. The integration test asserts the exact 11-name set.

- [ ] **Step 2: Write the `tools/call` dispatcher (one full exemplar + exact table)**

```rust
fn text_result(text: String) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "isError": false })
}
fn tool_error(msg: String) -> Value {
    json!({ "content": [{ "type": "text", "text": msg }], "isError": true })
}

async fn call_tool(name: &str, a: &Value) -> Value {
    let s = |k: &str| a.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    let b = |k: &str| a.get(k).and_then(|v| v.as_bool()).unwrap_or(false);
    let n = |k: &str, d: u64| a.get(k).and_then(|v| v.as_u64()).unwrap_or(d);
    match name {
        "list_instances" => match portside::docker::list_instances().await {
            Ok(v) => text_result(serde_json::to_string(&v).unwrap()),
            Err(e) => tool_error(e),
        },
        "create_instance" => {
            let engine = s("engine");
            if engine.is_empty() { return tool_error("engine is required".to_string()); }
            let (tag, port) = match super::cmds::resolve_create(&engine, &s("tag"), a.get("port").and_then(|v| v.as_u64()).unwrap_or(0) as u16) {
                Ok(v) => v,
                Err(e) => return tool_error(e),
            };
            let lan = b("lan");
            let bind = if lan { "0.0.0.0" } else { "127.0.0.1" }.to_string();
            let inst = match portside::docker::create_instance(engine, tag, port, Some(bind), Some(s("password"))).await {
                Ok(i) => i,
                Err(e) => return tool_error(e),
            };
            let wait = n("wait", 60);
            if wait > 0 {
                if let Err(e) = portside::health::wait_healthy(&inst.id, wait).await {
                    return tool_error(format!("created {} but {e}", inst.id));
                }
            }
            let fresh = portside::state::load_instance(&inst.id).unwrap_or(inst);
            let lan_ip = if lan { portside::certs::lan_ip() } else { None };
            let info = portside::connect::for_instance(&fresh, lan_ip.as_deref(), false);
            text_result(serde_json::to_string(&json!({
                "id": fresh.id, "engine": fresh.engine, "tag": fresh.tag,
                "port": fresh.port, "status": fresh.status,
                "localhost_url": info.localhost_url, "lan_url": info.lan_url,
                "tls": info.tls,
                "summary": format!("{} {}:{} ready — {}", fresh.engine, fresh.port, fresh.tag, info.localhost_url),
            })).unwrap())
        }
        // Remaining arms follow the identical match-call-render pattern:
        // get_connection -> load_instance + lan_ip + for_instance(id, lan, strict) -> text_result(urls json)
        // create_database -> schemas::create_database(id, name) -> ok:true / tool_error
        // list_databases  -> schemas::list_databases(id) -> text_result(json array)
        // instance_health -> wait>0 ? wait_healthy : health -> text_result(json)
        // container_logs  -> health::container_logs(id, tail default 200) -> text_result(json {logs})
        // start_instance  -> docker::start_instance -> ok:true / tool_error
        // stop_instance   -> docker::stop_instance -> ok:true / tool_error
        // remove_instance -> docker::remove_instance(id, wipe default false) -> ok:true / tool_error
        // list_catalog    -> catalog::bundled_catalog() -> text_result(json)
        _ => return tool_error(format!("unknown tool {name}")),
    }
}
```

Hmm — unknown tool must be a PROTOCOL error (-32602), not isError result. Restructure: `call_tool` returns `Result<Value, String>`; `Err` → `err(id, -32602, msg)`. Rewrite the match so every arm is `match core_call.await { Ok(v) => text_result(...), Err(e) => tool_error(e) }` and the fallthrough is `return Err(format!("unknown tool {name}"))`. State this explicitly in code (not as a comment-to-fill: write all 11 arms fully — each is 4-6 lines following the list_instances exemplar; the table above is the exact mapping).

Also `resolve_create` is `pub` in cmds.rs — confirm: Task 4 declares `pub fn resolve_create`. And `mcp.rs` accesses it as `super::cmds::resolve_create` — but main.rs declares `mod cmds; mod mcp;` making them siblings: path is `crate::cmds::resolve_create`. Use that.

`Cmd::Mcp` wiring in main.rs:

```rust
if matches!(cli.cmd, cmds::Cmd::Mcp) {
    mcp::serve().await;
    return;
}
```

placed after parse, before `cmds::run`. And REMOVE the Task 3 placeholder `_ => usage("lifecycle...")` handling of Mcp (Task 4's match must have a `Cmd::Mcp => unreachable!("handled in main")` arm — add that in this task's main.rs edit; clap requires exhaustive match).

- [ ] **Step 3: Write the serve loop**

```rust
pub async fn serve() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
    let stdin = tokio::io::stdin();
    let mut lines = tokio::io::BufReader::new(stdin).lines();
    let mut stdout = tokio::io::stdout();
    while let Ok(Some(line)) = lines.next_line().await {
        if line.trim().is_empty() { continue; }
        let resp: Option<String> = match serde_json::from_str::<Req>(&line) {
            Err(_) => Some(serde_json::to_string(&json!({ "jsonrpc": "2.0", "id": Value::Null,
                "error": { "code": -32700, "message": "parse error" } })).unwrap()),
            Ok(req) => handle(req).await,
        };
        if let Some(r) = resp {
            stdout.write_all(r.as_bytes()).await.ok();
            stdout.write_all(b"\n").await.ok();
            stdout.flush().await.ok();
        }
    }
}

async fn handle(req: Req) -> Option<String> {
    let null = Value::Null;
    let id = req.id.as_ref().unwrap_or(&null);
    match req.method.as_str() {
        "initialize" => Some(ok(id, json!({
            "protocolVersion": "2024-11-05",
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "portside", "version": env!("CARGO_PKG_VERSION") }
        }))),
        "tools/list" => Some(ok(id, super::mcp_tools_list())),
        "tools/call" => {
            let name = req.params.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let args = req.params.get("arguments").cloned().unwrap_or(json!({}));
            match call_tool(name, &args).await {
                Ok(v) => Some(ok(id, v)),
                Err(e) => Some(err(id, -32602, &e)),
            }
        }
        m if m.starts_with("notifications/") => None,
        _ if req.id.is_none() => None, // unknown notification: swallow
        _ => Some(err(id, -32601, "method not found")),
    }
}
```

(`tools_list()` is the fn from Step 1 — name it `pub(crate) fn tools_list()` in mcp.rs and call directly.)

Unused import check: `Serialize` unused — drop it (`use serde::Deserialize;` only).

- [ ] **Step 4: Write the integration test `src-tauri/tests/mcp_stdio.rs`**

```rust
//! MCP stdio smoke test: initialize -> tools/list -> tools/call(list_catalog).
//! No Docker needed (list_catalog is a pure read).
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::time::Duration;

fn bin() -> std::path::PathBuf {
    let mut p = std::env::current_exe().unwrap();
    p.pop(); // deps -> debug
    if p.ends_with("deps") { p.pop(); }
    p.join(format!("portside-cli{}", std::env::consts::EXE_SUFFIX))
}

fn send(stream: &mut impl Write, v: &Value) {
    writeln!(stream, "{}", v).unwrap();
}

#[test]
fn mcp_handshake_lists_eleven_tools_and_calls_catalog() {
    let mut child = Command::new(bin())
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn portside-cli mcp");
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();

    send(&mut stdin, &json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {} }));
    out.read_line(&mut line).unwrap();
    let init: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(init["result"]["serverInfo"]["name"], "portside");

    line.clear();
    send(&mut stdin, &json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {} }));
    out.read_line(&mut line).unwrap();
    let list: Value = serde_json::from_str(&line).unwrap();
    let names: Vec<&str> = list["result"]["tools"].as_array().unwrap()
        .iter().map(|t| t["name"].as_str().unwrap()).collect();
    for expected in ["list_instances", "create_instance", "get_connection", "create_database",
        "list_databases", "instance_health", "container_logs", "start_instance",
        "stop_instance", "remove_instance", "list_catalog"] {
        assert!(names.contains(&expected), "missing tool {expected}");
    }
    assert_eq!(names.len(), 11);

    line.clear();
    send(&mut stdin, &json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call",
        "params": { "name": "list_catalog", "arguments": {} } }));
    out.read_line(&mut line).unwrap();
    let call: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(call["result"]["isError"], false);
    assert!(call["result"]["content"][0]["text"].as_str().unwrap().contains("postgres:17"));

    drop(stdin);
    let _ = child.wait_timeout(Duration::from_secs(5));
    let _ = child.kill();
}

#[test]
fn mcp_unknown_tool_is_protocol_error() {
    // ... same spawn shape, single tools/call with name "nope" ...
}
```

Write the second test fully (same spawn/read one line, assert `error.code == -32602`):

```rust
#[test]
fn mcp_unknown_tool_is_protocol_error() {
    let mut child = Command::new(bin())
        .arg("mcp").stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null())
        .spawn().expect("spawn portside-cli mcp");
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());
    send(&mut stdin, &json!({ "jsonrpc": "2.0", "id": 9, "method": "tools/call",
        "params": { "name": "nope", "arguments": {} } }));
    let mut line = String::new();
    out.read_line(&mut line).unwrap();
    let v: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(v["error"]["code"], -32602);
    drop(stdin);
    let _ = child.kill();
}
```

`wait_timeout` needs the `wait-timeout` crate — NOT available. Replace: after `drop(stdin)` the server sees EOF and exits on its own; then `child.wait()` (blocking, fine — server exits at EOF). Use `child.wait()` with no timeout in both tests. Fix Step 4 code accordingly: remove `Duration`/`wait_timeout`, end each test with `drop(stdin); child.wait().ok();`.

- [ ] **Step 5: Run the MCP tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --test mcp_stdio`
Expected: 2 tests PASS. (Builds the `portside-cli` binary automatically as `CARGO_BIN_EXE` dep? No — integration tests do NOT auto-build binaries; `cargo test` builds all targets including bins by default, so `target/debug/portside-cli` exists. The `bin()` helper path logic covers `target/debug/` layout.)

- [ ] **Step 6: Amend the spec (rmcp -> hand-rolled) + fix tool count**

In `docs/superpowers/specs/2026-10-11-agent-interface-design.md`: replace "plus `portside mcp` (stdio MCP via `rmcp`)" with "plus `portside-cli mcp` (hand-rolled newline-delimited JSON-RPC stdio server on serde_json+tokio: zero new protocol deps, exact 11-tool surface, integration-tested)". Replace "10 tools" with "11 tools" everywhere (2 spots: MCP surface intro + testing section).

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/bin/portside-cli/mcp.rs src-tauri/src/bin/portside-cli/main.rs src-tauri/src/bin/portside-cli/cmds.rs src-tauri/tests/mcp_stdio.rs docs/superpowers/specs/2026-10-11-agent-interface-design.md
git commit -m "feat(mcp): stdio MCP server with 11 tools + integration tests"
```

---

### Task 6: Docs, host configs, release artifact, full verification

**Files:**
- Modify: `README.md` (new "Agents (CLI + MCP)" section)
- Modify: `.github/workflows/release.yml` (build + upload CLI per OS)
- Test: full `cargo test`, `npx vitest run`, Docker e2e (when available)

**Interfaces:**
- Consumes: finished `portside-cli` binary + MCP server.
- Produces: user-facing docs + release artifacts; final green test matrix.

- [ ] **Step 1: README section (append after "How you use it")**

```markdown
## Agents (CLI + MCP)

Heads-up control for agents and scripts — same instances, same Docker,
same SQLite as the desktop app.

```bash
cargo build --release --manifest-path src-tauri/Cargo.toml --bin portside-cli
./target/release/portside-cli create --engine postgres --tag 17 --wait 60 --json
./target/release/portside-cli conn <id> --json
./target/release/portside-cli db create <id> demo --json
./target/release/portside-cli rm <id> --wipe
```

opencode (`opencode.json`):

```json
{ "mcp": { "portside": { "type": "local", "command": ["C:/path/to/portside-cli.exe", "mcp"] } } }
```

Claude Code (`.mcp.json` in your project):

```json
{ "mcpServers": { "portside": { "command": "C:/path/to/portside-cli.exe", "args": ["mcp"] } } }
```

Prompt your agent: "use portside to stand up postgres 17 with a
database called demo and give me the connect string."
```

(Adjust binary path per OS; `--json` everywhere for agents.)

- [ ] **Step 2: Release workflow — upload CLI alongside installers**

Append after the tauri-action step in `.github/workflows/release.yml`:

```yaml
      - name: build portside-cli
        run: cargo build --release --manifest-path src-tauri/Cargo.toml --bin portside-cli

      - name: upload portside-cli to release
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        run: |
          ASSET="portside-cli-${{ runner.os }}-${{ matrix.args }}"
          ASSET="${ASSET// /-}"
          ASSET="${ASSET//-/x}"
          cp src-tauri/target/release/portside-cli* "./$ASSET" 2>/dev/null || cp src-tauri/target/release/portside-cli.exe "./$ASSET.exe"
          gh release upload "${{ github.ref_name }}" "./$ASSET"* --clobber
```

Hmm — that ASSET munging is fragile nonsense. Replace with explicit per-OS naming:

```yaml
      - name: build portside-cli
        run: cargo build --release --manifest-path src-tauri/Cargo.toml --bin portside-cli ${{ matrix.args }}

      - name: upload portside-cli to draft release
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        shell: bash
        run: |
          if [ "${{ runner.os }}" = "Windows" ]; then BIN=src-tauri/target/release/portside-cli.exe; OUT=portside-cli-windows-amd64.exe;
          elif [ "${{ runner.os }}" = "macOS" ]; then BIN=src-tauri/target/release/portside-cli; OUT=portside-cli-macos-${{ matrix.args }}; OUT=${OUT// /-};
          else BIN=src-tauri/target/release/portside-cli; OUT=portside-cli-linux-amd64; fi
          # mac target dirs differ per --target; fall back to a find:
          [ -f "$BIN" ] || BIN=$(find src-tauri/target -name 'portside-cli*' -type f -path '*release*' | head -n 1)
          cp "$BIN" "./$OUT"
          gh release upload "${{ github.ref_name }}" "./$OUT" --clobber
```

Keep it — `find` fallback covers `--target` output dirs; `gh` exists on all runners.

- [ ] **Step 3: Full offline verification**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: PASS (unit + mcp_stdio; e2e ignored).

Run: `npx vitest run`
Expected: PASS.

- [ ] **Step 4: Docker verification (only when a daemon is up)**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -- --ignored e2e`
Expected: PASS (proves the core refactor didn't change runtime behavior).

Plus the Task 4 CLI smoke on redis:56380 if not already done.

- [ ] **Step 5: Commit**

```bash
git add README.md .github/workflows/release.yml
git commit -m "docs: agent CLI+MCP usage, host configs, release artifact"
```

---

## Self-review

- **Spec coverage:** CLI surface (list/catalog/create/start/stop/rm/db/conn/logs/health/mcp) → Tasks 3+4+5. MCP 11 tools + create returns localhost+lan URLs → Task 5. Remote story via lan_url → Tasks 2+5. Same SQLite/Docker state → Task 1 (no path changes). Exit codes + stderr JSON → Task 3 output.rs. Rollback semantics → untouched core, Task 4 surfaces errors. Tests (unit, --json golden via integration test, gated e2e, MCP handshake) → Tasks 2/4/5/6. Docs + mcp.json/opencode snippet + release artifact → Task 6.
- **Placeholder scan:** no TBD/TODO; every step has exact code/commands; per-tool MCP mapping is an explicit table with a complete exemplar; release YAML is literal.
- **Type consistency:** `resolve_create(&str,&str,u16)->Result<(String,u16),String>` used identically in Tasks 4+5. `ConnectionInfo{localhost_url:String,lan_url:Option<String>,tls:bool}` in Tasks 2+3+5. `wait_healthy(&str,u64)->Result<Health,String>` in Tasks 2+3+5. `Cmd::Mcp` handled in main (Task 5), match-exhaustive. `portside::` paths match lib.rs module names.
- Fixes applied inline while writing: spec "10 tools"→11 counted and corrected; `wait_timeout` removed (no such dep); postgres/mysql format arms rewritten cleanly; `super::cmds` corrected to `crate::cmds`; clap structs placed in cmds.rs as `pub`.
