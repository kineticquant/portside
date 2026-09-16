//! External database import: register servers Portside didn't create.
//!
//! Two flavors: `imported` rows are bare connections (pgAdmin servers,
//! hand-typed host/port/creds) with no lifecycle; `adopted` rows wrap
//! pre-existing Docker containers so start/stop/remove keep working.
//! Passwords land in SQLite plaintext, same tradeoff as managed rows.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ProbeResult {
    pub ok: bool,
    pub version: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Adoptable {
    pub container: String,
    pub engine: String,
    pub tag: String,
    pub port: Option<u16>,
    pub volume: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PgServer {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub database: String,
}

fn check_engine(engine: &str) -> Result<(), String> {
    match engine {
        "mysql" | "mariadb" | "postgres" | "redis" | "valkey" => Ok(()),
        other => Err(format!("unsupported engine {other}")),
    }
}

fn tls_for(ssl: &str) -> bool {
    ssl == "require"
}

/// Guess (engine, tag) from a Docker image reference. Unknown images
/// yield ("", "") so callers can skip them.
fn guess_engine(image: &str) -> (&str, &str) {
    let lower = image.to_lowercase();
    let engine = if lower.contains("postgres") || lower.contains("pgvector") {
        "postgres"
    } else if lower.contains("mariadb") {
        "mariadb"
    } else if lower.contains("mysql") {
        "mysql"
    } else if lower.contains("valkey") {
        "valkey"
    } else if lower.contains("redis") {
        "redis"
    } else {
        return ("", "");
    };
    let tag = match image.rsplit_once(':') {
        Some((_, t)) if !t.contains('/') => t,
        _ => "latest",
    };
    (engine, tag)
}

fn standard_port(engine: &str) -> u16 {
    match engine {
        "postgres" => 5432,
        "mysql" | "mariadb" => 3306,
        _ => 6379,
    }
}

async fn probe_inner(
    engine: &str,
    host: &str,
    port: u16,
    user: &str,
    password: &str,
    tls: bool,
) -> Result<Option<String>, String> {
    let user = if user.is_empty() {
        match engine {
            "postgres" => "postgres",
            _ => "root",
        }
    } else {
        user
    };
    match engine {
        "mysql" | "mariadb" => {
            let url = crate::schemas::mysql_url(host, port, "mysql", user, password, tls);
            let pool = sqlx::MySqlPool::connect(&url)
                .await
                .map_err(|e| e.to_string())?;
            let row: (String,) = sqlx::query_as("SELECT VERSION()")
                .fetch_one(&pool)
                .await
                .map_err(|e| e.to_string())?;
            Ok(Some(row.0))
        }
        "postgres" => {
            let url = crate::schemas::pg_url(host, port, "postgres", user, password, tls);
            let pool = sqlx::PgPool::connect(&url)
                .await
                .map_err(|e| e.to_string())?;
            let row: (String,) = sqlx::query_as("SHOW server_version")
                .fetch_one(&pool)
                .await
                .map_err(|e| e.to_string())?;
            Ok(Some(row.0))
        }
        "redis" | "valkey" => {
            let mut con = crate::schemas::redis_conn(host, port, password, tls).await?;
            let info: String = redis::cmd("INFO")
                .arg("server")
                .query_async(&mut con)
                .await
                .map_err(|e| e.to_string())?;
            Ok(info
                .lines()
                .find_map(|l| l.strip_prefix("redis_version:").map(|v| v.trim().to_string())))
        }
        other => Err(format!("unsupported engine {other}")),
    }
}

/// Test a connection without saving anything. Unknown engines are a hard
/// error; unreachable/bad-auth servers come back as ok:false so the UI
/// can show the message inline next to Test.
#[tauri::command]
pub async fn probe_connection(
    engine: String,
    host: String,
    port: u16,
    user: String,
    password: String,
    ssl: String,
) -> Result<ProbeResult, String> {
    check_engine(&engine)?;
    let tls = tls_for(&ssl);
    match tokio::time::timeout(
        std::time::Duration::from_secs(8),
        probe_inner(&engine, &host, port, &user, &password, tls),
    )
    .await
    {
        Ok(Ok(version)) => Ok(ProbeResult { ok: true, version, error: None }),
        Ok(Err(e)) => Ok(ProbeResult { ok: false, version: None, error: Some(e) }),
        Err(_) => Ok(ProbeResult {
            ok: false,
            version: None,
            error: Some("connection timed out after 8s".to_string()),
        }),
    }
}

/// Save an external server after a successful probe. The row is the whole
/// feature: health, browsing, and connect strings flow from it.
#[tauri::command]
pub async fn import_external(
    engine: String,
    host: String,
    port: u16,
    user: String,
    password: String,
    ssl: String,
) -> Result<crate::docker::Instance, String> {
    check_engine(&engine)?;
    if host.trim().is_empty() {
        return Err("host is required".to_string());
    }
    let probe = probe_connection(
        engine.clone(),
        host.clone(),
        port,
        user.clone(),
        password.clone(),
        ssl.clone(),
    )
    .await?;
    if !probe.ok {
        return Err(probe.error.unwrap_or_else(|| "probe failed".to_string()));
    }
    let id = uuid::Uuid::new_v4().to_string().replace('-', "")[..12].to_string();
    let inst =
        crate::docker::Instance::external(&id, &engine, &host, port, &user, &password, &ssl);
    crate::state::save_instance(&inst)?;
    Ok(inst)
}

/// Forget an imported row. Deletes Portside's record only; the server
/// itself is never touched.
#[tauri::command]
pub async fn forget_instance(id: String) -> Result<(), String> {
    let inst = crate::state::load_instance(&id)?;
    if inst.origin != "imported" {
        return Err("only imported rows can be forgotten".to_string());
    }
    crate::state::delete_instance(&id)
}

fn first_host_port(ports: &std::collections::HashMap<String, Option<Vec<bollard::models::PortBinding>>>) -> Option<u16> {
    ports
        .values()
        .flatten()
        .flatten()
        .filter_map(|b| b.host_port.as_deref())
        .filter_map(|p| p.parse::<u16>().ok())
        .next()
}

fn first_volume_name(mounts: &[bollard::models::MountPoint]) -> String {
    mounts
        .iter()
        .filter_map(|m| m.name.clone())
        .next()
        .unwrap_or_default()
}

/// Containers on this daemon that look like databases Portside didn't
/// create. Our own `portside-*` containers and non-DB images are skipped.
#[tauri::command]
pub async fn list_adoptable() -> Result<Vec<Adoptable>, String> {
    let docker = crate::docker::connect()?;
    let containers = docker
        .list_containers(Some(bollard::container::ListContainersOptions::<String> {
            all: true,
            ..Default::default()
        }))
        .await
        .map_err(|e| format!("container list failed: {e}"))?;
    let mut out = Vec::new();
    for c in containers {
        let name = c
            .names
            .as_deref()
            .and_then(|n| n.first())
            .map(|n| n.trim_start_matches('/').to_string())
            .unwrap_or_default();
        if name.is_empty() || name.starts_with("portside-") {
            continue;
        }
        let image = c.image.as_deref().unwrap_or("");
        let (engine, tag) = guess_engine(image);
        if engine.is_empty() {
            continue;
        }
        // Ports and volumes need an inspect per candidate.
        let info = docker
            .inspect_container(&name, None)
            .await
            .map_err(|e| format!("inspect failed ({name}): {e}"))?;
        let port = info
            .network_settings
            .as_ref()
            .and_then(|n| n.ports.as_ref())
            .and_then(first_host_port);
        let volume = info
            .mounts
            .as_deref()
            .map(first_volume_name)
            .unwrap_or_default();
        let status = match info
            .state
            .as_ref()
            .and_then(|s| s.running)
            .unwrap_or(false)
        {
            true => "running",
            false => "stopped",
        }
        .to_string();
        out.push(Adoptable {
            container: name,
            engine: engine.to_string(),
            tag: tag.to_string(),
            port,
            volume,
            status,
        });
    }
    Ok(out)
}

/// Bring a pre-existing container under management. Port falls back to
/// the engine default when the container publishes none.
#[tauri::command]
pub async fn adopt_container(container: String) -> Result<crate::docker::Instance, String> {
    let docker = crate::docker::connect()?;
    let info = docker
        .inspect_container(&container, None)
        .await
        .map_err(|e| format!("inspect failed ({container}): {e}"))?;
    let image = info
        .config
        .as_ref()
        .and_then(|c| c.image.as_deref())
        .unwrap_or("");
    let (engine, tag) = guess_engine(image);
    if engine.is_empty() {
        return Err(format!("{container} does not look like a supported database (image: {image})"));
    }
    let port = info
        .network_settings
        .as_ref()
        .and_then(|n| n.ports.as_ref())
        .and_then(first_host_port)
        .unwrap_or_else(|| standard_port(engine));
    let volume = info
        .mounts
        .as_deref()
        .map(first_volume_name)
        .unwrap_or_default();
    let status = match info
        .state
        .as_ref()
        .and_then(|s| s.running)
        .unwrap_or(false)
    {
        true => "running",
        false => "stopped",
    };
    let id = uuid::Uuid::new_v4().to_string().replace('-', "")[..12].to_string();
    let inst = crate::docker::Instance::adopted(
        &id,
        engine,
        tag,
        port,
        &container,
        &volume,
        status,
    );
    crate::state::save_instance(&inst)?;
    Ok(inst)
}

fn pgadmin_candidate_dirs() -> Vec<std::path::PathBuf> {
    let mut dirs = Vec::new();
    #[cfg(windows)]
    if let Some(roaming) = dirs::data_dir() {
        dirs.push(roaming.join("pgadmin").join("pgadmin4.db"));
    }
    #[cfg(not(windows))]
    {
        if let Some(home) = dirs::home_dir() {
            dirs.push(home.join(".pgadmin").join("pgadmin4.db"));
            dirs.push(home.join(".config").join("pgadmin").join("pgadmin4.db"));
        }
        if let Some(data) = dirs::data_dir() {
            dirs.push(data.join("pgadmin").join("pgadmin4.db"));
        }
    }
    dirs
}

fn parse_pgadmin_servers(path: &std::path::Path) -> Result<Vec<PgServer>, String> {
    let conn = rusqlite::Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .map_err(|e| format!("cannot open pgAdmin db: {e}"))?;
    let mut stmt = conn
        .prepare("SELECT name, host, port, username, maintenance_db FROM server ORDER BY name")
        .map_err(|e| format!("pgAdmin server table unreadable: {e}"))?;
    let rows = stmt
        .query_map([], |row| {
            let port: i64 = row.get(2)?;
            let username: Option<String> = row.get(3)?;
            let database: Option<String> = row.get(4)?;
            Ok(PgServer {
                name: row.get(0)?,
                host: row.get(1)?,
                port: port as u16,
                username: username.unwrap_or_default(),
                database: database.unwrap_or_else(|| "postgres".to_string()),
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
}

/// Server entries from pgAdmin's own config (host/port/user only:
/// pgAdmin encrypts saved passwords, so the UI prompts once per row).
#[tauri::command]
pub async fn list_pgadmin_servers() -> Result<Vec<PgServer>, String> {
    for path in pgadmin_candidate_dirs() {
        if path.exists() {
            match parse_pgadmin_servers(&path) {
                Ok(servers) => return Ok(servers),
                Err(e) => return Err(e),
            }
        }
    }
    Err("pgAdmin server list not found on this machine".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block_on<F: std::future::Future>(f: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(f)
    }

    #[test]
    fn probe_rejects_unknown_engine() {
        let r = block_on(probe_connection(
            "oracle".to_string(),
            "h".to_string(),
            1,
            "u".to_string(),
            "p".to_string(),
            "off".to_string(),
        ));
        assert!(r.is_err());
    }

    #[test]
    fn pgadmin_row_parsing_reads_name_host_port() {
        let dir = std::env::temp_dir().join(format!("ps-pgadmin-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("pgadmin4.db");
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE server(id INTEGER PRIMARY KEY, name TEXT, host TEXT,
             port INTEGER, maintenance_db TEXT, username TEXT);",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO server(name, host, port, maintenance_db, username)
             VALUES ('local pg', '127.0.0.1', 5433, 'postgres', 'postgres')",
            [],
        )
        .unwrap();
        drop(conn);
        let servers = parse_pgadmin_servers(&path).unwrap();
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].name, "local pg");
        assert_eq!(servers[0].host, "127.0.0.1");
        assert_eq!(servers[0].port, 5433);
        assert_eq!(servers[0].username, "postgres");
        assert_eq!(servers[0].database, "postgres");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn engine_guess_spots_db_images() {
        assert_eq!(guess_engine("postgres:15"), ("postgres", "15"));
        assert_eq!(
            guess_engine("myregistry:5000/pgvector/pgvector:pg16"),
            ("postgres", "pg16")
        );
        assert_eq!(guess_engine("mysql:8.4"), ("mysql", "8.4"));
        assert_eq!(guess_engine("redis:7"), ("redis", "7"));
        assert_eq!(guess_engine("nginx:latest"), ("", ""));
    }
}
