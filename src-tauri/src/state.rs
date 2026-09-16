use rusqlite::{params, Connection};
use crate::docker::Instance;

pub fn data_dir() -> std::path::PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("portside")
}

pub fn db() -> Result<Connection, String> {
    let dir = data_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let conn = Connection::open(dir.join("portside.db")).map_err(|e| e.to_string())?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS instances(
          id TEXT PRIMARY KEY, engine TEXT, tag TEXT, port INTEGER,
          container TEXT, volume TEXT, status TEXT,
          bind_ip TEXT NOT NULL DEFAULT '127.0.0.1',
          password TEXT NOT NULL DEFAULT '');",
    )
    .map_err(|e| e.to_string())?;
    // Migrations for DBs created before bind_ip/password existed.
    for stmt in [
        "ALTER TABLE instances ADD COLUMN bind_ip TEXT NOT NULL DEFAULT '127.0.0.1'",
        "ALTER TABLE instances ADD COLUMN password TEXT NOT NULL DEFAULT ''",
    ] {
        let _ = conn.execute_batch(stmt);
    }
    Ok(conn)
}

fn row_to_instance(row: &rusqlite::Row) -> rusqlite::Result<Instance> {
    Ok(Instance {
        id: row.get(0)?,
        engine: row.get(1)?,
        tag: row.get(2)?,
        port: row.get(3)?,
        container: row.get(4)?,
        volume: row.get(5)?,
        status: row.get(6)?,
        bind_ip: row.get(7)?,
        password: row.get(8)?,
    })
}

pub fn save_instance(inst: &Instance) -> Result<(), String> {
    db()?.execute(
        "INSERT OR REPLACE INTO instances(id, engine, tag, port, container, volume, status, bind_ip, password)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            inst.id,
            inst.engine,
            inst.tag,
            inst.port,
            inst.container,
            inst.volume,
            inst.status,
            inst.bind_ip,
            inst.password
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn load_instance(id: &str) -> Result<Instance, String> {
    let conn = db()?;
    conn.query_row(
        "SELECT id, engine, tag, port, container, volume, status, bind_ip, password FROM instances WHERE id = ?1",
        params![id],
        row_to_instance,
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => format!("instance {id} not found"),
        other => other.to_string(),
    })
}

pub fn list_saved_instances() -> Result<Vec<Instance>, String> {
    let conn = db()?;
    let mut stmt = conn
        .prepare("SELECT id, engine, tag, port, container, volume, status, bind_ip, password FROM instances ORDER BY id")
        .map_err(|e| e.to_string())?;
    let rows: Vec<Instance> = stmt
        .query_map([], row_to_instance)
        .map_err(|e| e.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| e.to_string())?;
    Ok(rows)
}

pub fn update_status(id: &str, status: &str) -> Result<(), String> {
    let n = db()?
        .execute(
            "UPDATE instances SET status = ?1 WHERE id = ?2",
            params![status, id],
        )
        .map_err(|e| e.to_string())?;
    if n == 0 {
        return Err(format!("instance {id} not found"));
    }
    Ok(())
}

pub fn delete_instance(id: &str) -> Result<(), String> {
    db()?.execute("DELETE FROM instances WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}
