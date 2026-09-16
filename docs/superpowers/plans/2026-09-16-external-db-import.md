# External DB Import Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Register databases Portside didn't create and manage them in-app (health, browse, connect strings).

**Architecture:** One `instances` table with an `origin` column. All existing rows backfill to `managed`, so current behavior is untouched. New `import.rs` backend module holds the five new commands plus pgAdmin parsing. URL builders gain host/user params and every call site switches to row values.

**Tech Stack:** Rust (Tauri commands, rusqlite, sqlx, bollard, redis), React + vitest frontend.

**Spec:** `docs/superpowers/specs/2026-09-16-external-db-import-design.md`

## Global Constraints

- Existing `managed` rows must behave exactly as before (loopback, engine-default user, bind_ip TLS rule).
- Passwords stay plaintext in SQLite, same as today.
- No `TBD`/`TODO` placeholders; every task ends green + committed.
- Frontend copy: plain language, no em dashes, no jargon without explanation.

---

## File map

- Modify: `src-tauri/src/docker.rs` (Instance fields + constructors)
- Modify: `src-tauri/src/state.rs` (migration, row mapping, save/load)
- Modify: `src-tauri/src/schemas.rs` (host/user params on URL builders + call sites)
- Modify: `src-tauri/src/health.rs` (per-origin health, TCP host, logs guard)
- Create: `src-tauri/src/import.rs` (probe, import, adopt, pgAdmin, forget)
- Modify: `src-tauri/src/main.rs` (`mod import;` + 6 commands)
- Modify: `src/lib/tauri.ts` (types, api, connectString)
- Create: `src/components/ImportDialog.tsx` (3-tab dialog)
- Modify: `src/components/InstanceList.tsx` (badges, conditional buttons, Forget)
- Modify: `src/App.tsx` (Import button, dialog state, forget handler)

## Shared signatures (locked)

```rust
// docker.rs
pub struct Instance {
    pub id: String, pub engine: String, pub tag: String, pub port: u16,
    pub container: String, pub volume: String, pub status: String,
    pub bind_ip: String, pub password: String,
    pub origin: String,   // "managed" | "imported" | "adopted"
    pub host: String,     // connection host ("127.0.0.1" for managed)
    pub db_user: String,  // "" = engine default (root / postgres)
    pub ssl: String,      // "" (managed rule) | "off" | "require"
}
impl Instance {
    pub fn new(id: &str, engine: &str, tag: &str, port: u16, bind_ip: &str, password: &str) -> Self;
    pub fn external(id: &str, engine: &str, host: &str, port: u16, user: &str, password: &str, ssl: &str) -> Self;
    pub fn adopted(id: &str, engine: &str, tag: &str, port: u16, container: &str, volume: &str) -> Self;
    pub fn tls(&self) -> bool;      // managed: bind_ip rule (unchanged); else ssl == "require"
    pub fn db_user_or_default(&self) -> &str; // db_user, or "root"/"postgres" when empty
}

// schemas.rs
pub fn mysql_url(host: &str, port: u16, db: &str, user: &str, password: &str, tls: bool) -> String;
pub fn pg_url(host: &str, port: u16, db: &str, user: &str, password: &str, tls: bool) -> String;
pub fn redis_url(host: &str, port: u16, password: &str, tls: bool) -> String;
pub fn redis_client(host: &str, port: u16, password: &str, tls: bool) -> Result<redis::Client, String>;
pub async fn redis_conn(host: &str, port: u16, password: &str, tls: bool) -> Result<...MultiplexedConnection, String>;

// import.rs
pub struct ProbeResult { pub ok: bool, pub version: Option<String>, pub error: Option<String> }
pub struct Adoptable { pub container: String, pub engine: String, pub tag: String, pub port: Option<u16>, pub volume: String, pub status: String }
pub struct PgServer { pub name: String, pub host: String, pub port: u16, pub username: String, pub database: String }
#[tauri::command] pub async fn probe_connection(engine: String, host: String, port: u16, user: String, password: String, ssl: String) -> Result<ProbeResult, String>;
#[tauri::command] pub async fn import_external(engine: String, host: String, port: u16, user: String, password: String, ssl: String) -> Result<Instance, String>;
#[tauri::command] pub async fn list_adoptable() -> Result<Vec<Adoptable>, String>;
#[tauri::command] pub async fn adopt_container(container: String) -> Result<Instance, String>;
#[tauri::command] pub async fn list_pgadmin_servers() -> Result<Vec<PgServer>, String>;
#[tauri::command] pub async fn forget_instance(id: String) -> Result<(), String>;
```

```typescript
// tauri.ts
export type Instance = { ...existing, origin: string; host: string; db_user: string; ssl: string };
export type ProbeResult = { ok: boolean; version: string | null; error: string | null };
export type Adoptable = { container: string; engine: string; tag: string; port: number | null; volume: string; status: string };
export type PgServer = { name: string; host: string; port: number; username: string; database: string };
// api gains: probe(engine, host, port, user, password, ssl), importExternal(...same), adoptable(), adopt(container), pgadmin(), forget(id)
```

---

### Task 1: Model + migration (backend)

**Files:**
- Modify: `src-tauri/src/docker.rs:13-55` (struct + impl)
- Modify: `src-tauri/src/state.rs` (CREATE TABLE, 3 ALTERs, row mapping cols 9-12, save/load SQL)
- Test: `cargo test --manifest-path src-tauri/Cargo.toml docker::`

**Interfaces:**
- Consumes: nothing new.
- Produces: `Instance::{external, adopted, db_user_or_default}`, widened `tls()`, 13-column rows.

- [ ] **Step 1: Failing test** — add to `docker.rs` tests mod:
```rust
#[test]
fn external_instance_uses_given_host_and_user() {
    let inst = Instance::external("e1", "postgres", "db.lan", 5432, "app", "pw", "require");
    assert_eq!(inst.origin, "imported");
    assert_eq!(inst.host, "db.lan");
    assert_eq!(inst.db_user_or_default(), "app");
    assert!(inst.tls());
}
#[test]
fn blank_user_falls_back_to_engine_default() {
    let inst = Instance::external("e2", "postgres", "db.lan", 5432, "", "pw", "off");
    assert_eq!(inst.db_user_or_default(), "postgres");
    assert!(!inst.tls());
}
#[test]
fn managed_tls_rule_is_unchanged() {
    let lan = Instance::new("a", "postgres", "17", 5433, "0.0.0.0", "pw");
    let local = Instance::new("b", "postgres", "17", 5434, "127.0.0.1", "pw");
    assert!(lan.tls());
    assert!(!local.tls());
}
```
- [ ] **Step 2: Run, expect FAIL** (`external`/`adopted`/`db_user_or_default` don't exist). Run: `cargo test --manifest-path src-tauri/Cargo.toml docker::`
- [ ] **Step 3: Implement** — add the 4 fields, `external()` (container/volume `""`, status `"external"`, bind_ip `"127.0.0.1"`, tag `"external"`), `adopted()` (origin `"adopted"`, host `"127.0.0.1"`, bind_ip `"127.0.0.1"`, db_user `""`, ssl `""`, status from caller), `db_user_or_default()`, widen `tls()` (managed keeps bind_ip rule; else `ssl == "require"`). Update `state.rs`: CREATE TABLE gains 4 columns with defaults (`'managed'`, `'127.0.0.1'`, `''`, `''`), 3 new ALTERs in the migration loop, `row_to_instance` reads indices 9-12, save/load SQL lists all 13 columns.
- [ ] **Step 4: Run, expect PASS.** Run: `cargo test --manifest-path src-tauri/Cargo.toml`
- [ ] **Step 5: Commit.** `git add src-tauri/src/docker.rs src-tauri/src/state.rs; git commit -m "feat: external/adopted instance model with migration"`

### Task 2: URL builders take host/user (backend)

**Files:**
- Modify: `src-tauri/src/schemas.rs:1-92` (5 signatures + bodies)
- Modify: call sites `schemas.rs:106,125,154,164,183,193,212,235` + `health.rs:45-80`
- Test: `cargo test --manifest-path src-tauri/Cargo.toml schemas::`

**Interfaces:**
- Consumes: `Instance::db_user_or_default`, `Instance.host` from Task 1.
- Produces: new builder signatures (locked above); every internal caller passes row values.

- [ ] **Step 1: Failing tests** — update the 6 existing URL tests to the new signatures and add:
```rust
#[test]
fn pg_url_uses_custom_user_and_host() {
    let url = pg_url("db.lan", 5433, "mydb", "app", "pw", false);
    assert_eq!(url, "postgres://app:pw@db.lan:5433/mydb");
}
#[test]
fn redis_url_uses_given_host() {
    let url = redis_url("db.lan", 6380, "", false);
    assert_eq!(url, "redis://db.lan:6380/");
}
```
- [ ] **Step 2: Run, expect FAIL** (signatures don't match).
- [ ] **Step 3: Implement** — `userinfo(user, …)` replaces hardcoded `"root"`/`"postgres"`; `redis_url`/`redis_client`/`redis_conn` take `host` (TLS branch uses it for `TcpTls.host`); all 8 call sites pass `&inst.host`-equivalent (`local_host()` for managed stays correct because managed rows store `127.0.0.1`) and `inst.db_user_or_default()`.
- [ ] **Step 4: Run, expect PASS** (full `cargo test`).
- [ ] **Step 5: Commit** `schemas.rs health.rs`, message `feat: connection URLs honor instance host and user`.

### Task 3: Per-origin health + logs guard (backend)

**Files:**
- Modify: `src-tauri/src/health.rs:85-112`
- Test: `cargo test --manifest-path src-tauri/Cargo.toml health::`

**Interfaces:**
- Consumes: `inst.origin`, `inst.host` (Task 1).
- Produces: `health()` truthful for all origins; `container_logs()` errors for `imported`.

- [ ] **Step 1: Failing test** — pure helper `container_running_for(origin, inspect_ok)`? Simpler: test the rule inline via a tiny `fn effective_container_running(origin: &str, inspected: bool) -> bool` with:
```rust
#[test]
fn imported_servers_have_no_container_concept() {
    assert!(effective_container_running("imported", false));
    assert!(!effective_container_running("managed", false));
    assert!(effective_container_running("managed", true));
}
```
- [ ] **Step 2: Run, expect FAIL.**
- [ ] **Step 3: Implement** — `health()`: `container_running` = `true` when `origin == "imported"`, else inspect result (covers adopted-stopped honestly); TCP connects to `format!("{}:{}", inst.host, inst.port)`; `container_logs()` returns `Err("no container logs for imported servers")` when `origin == "imported"`.
- [ ] **Step 4: Run, expect PASS.**
- [ ] **Step 5: Commit** `health.rs`, message `feat: health and logs honor instance origin`.

### Task 4: import.rs commands (backend)

**Files:**
- Create: `src-tauri/src/import.rs`
- Modify: `src-tauri/src/main.rs` (`mod import;` + 6 commands)
- Test: `cargo test --manifest-path src-tauri/Cargo.toml import::`

**Interfaces:**
- Consumes: `Instance::{external, adopted}`, `save_instance`/`delete_instance`, `probe` helpers, `Docker::list_containers`/`inspect_container` patterns from `docker.rs:497-538`.
- Produces: the 6 locked commands.

- [ ] **Step 1: Failing tests:**
```rust
#[test]
fn probe_rejects_unknown_engine() {
    let r = tokio_test_block_on(probe_connection("oracle", "h", 1, "u", "p", "off"));
    assert!(r.is_err());
}
#[test]
fn pgadmin_row_parsing_reads_name_host_port() {
    // build a temp SQLite db with a `server` table, parse it, assert fields
}
#[test]
fn engine_guess_spots_postgres_image() {
    assert_eq!(guess_engine("postgres:15"), ("postgres", "15"));
    assert_eq!(guess_engine("myregistry:5000/pgvector/pgvector:pg16"), ("postgres", "pg16"));
}
```
(`tokio_test_block_on`: use `tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(...)` — tokio full is already a dependency.)
- [ ] **Step 2: Run, expect FAIL** (module doesn't exist).
- [ ] **Step 3: Implement** — `probe_connection` per engine (mysql/mariadb: `MySqlPool::connect` + `SELECT 1`; postgres: same via PgPool; redis/valkey: `redis_conn` + PING; wrap version where cheap: postgres `SHOW server_version`, mysql `SELECT VERSION()`, redis `INFO server` → None on any failure, ok=false with message). `import_external` probes, maps failure to `Err`, saves `Instance::external` with a `uuid::Uuid::new_v4()` id on success. `list_adoptable` lists all containers, skips names starting with `portside-` and non-DB images (`guess_engine` matches postgres/mysql/mariadb/redis/valkey substrings), maps first public port binding and first volume mount. `adopt_container` inspects by name, reuses the same mapping, saves `Instance::adopted`. `list_pgadmin_servers` tries `%APPDATA%\pgadmin\pgadmin4.db` (win), `~/.pgadmin/pgadmin4.db`, `~/.config/pgadmin/pgadmin4.db`; read-only open (`sqlite3 URI` or copy to temp — use `Connection::open_with_flags(..., OpenFlags::SQLITE_OPEN_READ_ONLY)`), `SELECT name, host, port, username, maintenance_db FROM server`; missing file/table → `Err("pgAdmin server list not found …")`. `forget_instance` = `delete_instance` only.
- [ ] **Step 4: Run, expect PASS.**
- [ ] **Step 5: Commit** `import.rs main.rs`, message `feat: import/adopt/pgAdmin/probe backend commands`.

### Task 5: Frontend types + connect strings

**Files:**
- Modify: `src/lib/tauri.ts` (Instance, 3 new types, 6 api fns, connectString)
- Test: `src/lib/tauri.test.ts` (extend)

**Interfaces:**
- Consumes: backend field names from Task 1.
- Produces: typed api used by Tasks 6-7.

- [ ] **Step 1: Failing tests:**
```ts
it("points connect strings at the imported host and user", () => {
  expect(connectString({ engine: "postgres", port: 5432, host: "db.lan", db_user: "app", password: "pw", ssl: "off", bind_ip: "127.0.0.1" }))
    .toBe("postgres://app:pw@db.lan:5432/postgres");
});
it("requires TLS for imported ssl=require rows", () => {
  expect(connectString({ engine: "postgres", port: 5432, host: "db.lan", password: "pw", ssl: "require", bind_ip: "127.0.0.1" }))
    .toBe("postgres://postgres:pw@db.lan:5432/postgres?sslmode=require");
});
```
- [ ] **Step 2: Run `npx vitest run src/lib/tauri.test.ts`, expect FAIL.**
- [ ] **Step 3: Implement** — Instance gains 4 fields; `isLan` unchanged (managed only); `connectString` resolves `host = i.host || opts.host || "127.0.0.1"`, `user = i.db_user || default`, `tls = strict-relevant`: managed keeps bind_ip rule, imported/adopted use `ssl === "require"`; api adds the 6 invokes with exact backend names.
- [ ] **Step 4: Run, expect PASS** (+ `npx tsc --noEmit`).
- [ ] **Step 5: Commit** `tauri.ts tauri.test.ts`, message `feat: frontend types and connect strings for imported rows`.

### Task 6: ImportDialog component

**Files:**
- Create: `src/components/ImportDialog.tsx`
- Create: `src/components/ImportDialog.test.tsx`

**Interfaces:**
- Consumes: `api` from Task 5.
- Produces: `<ImportDialog open onClose onDone />` (calls `onDone()` after any successful save so App refreshes).

- [ ] **Step 1: Failing tests** — render with `open`, assert three tabs (Connection, Docker, pgAdmin); mock api.probe to resolve ok and assert Test shows version. (Mock `@tauri-apps/api/core`? No: pass api via props? Simplest: `vi.mock("../lib/tauri")` like `App.polling.test.tsx` does.)
- [ ] **Step 2: Run, expect FAIL** (file missing).
- [ ] **Step 3: Implement** — dialog reusing `ps-dialog` styles; Connection tab fields engine/host/port/user/password + SSL select (Off/Require) + Test button showing ok/version/error + Save (disabled until a successful probe); Docker tab lists `adoptable()` with Adopt per row; pgAdmin tab lists `pgadmin()` with per-row password field + Import; empty/error states per tab. No em dashes in copy.
- [ ] **Step 4: Run full `npm test` + tsc, expect PASS.**
- [ ] **Step 5: Commit**, message `feat: import dialog with connection/docker/pgAdmin tabs`.

### Task 7: Table, App wiring, Forget

**Files:**
- Modify: `src/components/InstanceList.tsx` (+ test), `src/App.tsx`

**Interfaces:**
- Consumes: `ImportDialog`, `api.forget`, origin/badges from Task 5.

- [ ] **Step 1: Failing tests** (`InstanceList.test.tsx`): imported row shows `Imported` badge + real host, hides Start/Inactivate/Wipe/Logs-touching buttons, shows Forget; managed row unchanged.
- [ ] **Step 2: Run, expect FAIL.**
- [ ] **Step 3: Implement** — badge column (`Local`/`Imported`/`Adopted`, adopted keeps LAN-TLS badge logic off since ssl is `""`); endpoint cell shows `host:port`; lifecycle group rendered only for managed/adopted; danger group: imported gets Forget (`window.confirm`, then `api.forget`); adopted keeps Remove/Wipe (Wipe disabled with title when `volume` empty); Cert button only when managed-LAN as today; LogsPanel shows "Logs aren't available for external servers." when selected row is imported; App gains Import button + dialog state + `handleForget` + refresh on done.
- [ ] **Step 4: Full `npm test` + tsc PASS.**
- [ ] **Step 5: Commit**, message `feat: origin badges, conditional actions, forget flow`.

### Task 8: Verify + push

- [ ] `npx tsc --noEmit` clean, `npm test` all green, `cargo test --manifest-path src-tauri/Cargo.toml` green (unit tests need no Docker; e2e-gated tests skip without daemon — confirm by running).
- [ ] Manual: import a live pgAdmin server, adopt a stray container, confirm Forget touches nothing.
- [ ] Push branch.
