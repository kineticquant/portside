pub fn mysql_url(host: &str, port: u16, db: &str) -> String {
    // Controller ruling 1: containers boot with MYSQL_ROOT_PASSWORD=portside,
    // so the URL must carry credentials (plan's passwordless URL would fail auth).
    format!("mysql://root:portside@{host}:{port}/{db}")
}

pub fn pg_url(host: &str, port: u16, db: &str) -> String {
    // Controller ruling 1: containers boot with POSTGRES_PASSWORD=portside.
    format!("postgres://postgres:portside@{host}:{port}/{db}")
}

pub fn redis_url(port: u16) -> String {
    // Controller ruling 2: redis/valkey containers set no password.
    format!("redis://127.0.0.1:{port}/")
}

fn check_db_name(name: &str) -> Result<(), String> {
    if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return Err("name must be [a-zA-Z0-9_]+".to_string());
    }
    Ok(())
}

#[tauri::command]
pub async fn list_databases(id: String) -> Result<Vec<String>, String> {
    let inst = crate::state::load_instance(&id)?;
    match inst.engine.as_str() {
        "mysql" | "mariadb" => {
            let pool = sqlx::MySqlPool::connect(&mysql_url("127.0.0.1", inst.port, "mysql"))
                .await
                .map_err(|e| e.to_string())?;
            let rows: Vec<(String,)> = sqlx::query_as("SHOW DATABASES")
                .fetch_all(&pool)
                .await
                .map_err(|e| e.to_string())?;
            Ok(rows
                .into_iter()
                .map(|r| r.0)
                .filter(|n| {
                    !["information_schema", "performance_schema", "sys", "mysql"]
                        .contains(&n.as_str())
                })
                .collect())
        }
        "postgres" => {
            let pool = sqlx::PgPool::connect(&pg_url("127.0.0.1", inst.port, "postgres"))
                .await
                .map_err(|e| e.to_string())?;
            let rows: Vec<(String,)> = sqlx::query_as(
                "SELECT datname FROM pg_database WHERE datistemplate = false ORDER BY 1",
            )
            .fetch_all(&pool)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows.into_iter().map(|r| r.0).collect())
        }
        "redis" | "valkey" => {
            let client =
                redis::Client::open(redis_url(inst.port)).map_err(|e| e.to_string())?;
            let mut con = client
                .get_multiplexed_async_connection()
                .await
                .map_err(|e| e.to_string())?;
            let n: usize = redis::cmd("DBSIZE")
                .query_async(&mut con)
                .await
                .map_err(|e| e.to_string())?;
            Ok(vec![format!("default ({n} keys)")])
        }
        other => Err(format!("unsupported engine {other}")),
    }
}

#[tauri::command]
pub async fn create_database(id: String, name: String) -> Result<(), String> {
    check_db_name(&name)?;
    let inst = crate::state::load_instance(&id)?;
    match inst.engine.as_str() {
        "mysql" | "mariadb" => {
            let pool = sqlx::MySqlPool::connect(&mysql_url("127.0.0.1", inst.port, "mysql"))
                .await
                .map_err(|e| e.to_string())?;
            sqlx::query(&format!("CREATE DATABASE `{name}`"))
                .execute(&pool)
                .await
                .map_err(|e| e.to_string())?;
            Ok(())
        }
        "postgres" => {
            let pool = sqlx::PgPool::connect(&pg_url("127.0.0.1", inst.port, "postgres"))
                .await
                .map_err(|e| e.to_string())?;
            sqlx::query(&format!("CREATE DATABASE \"{name}\""))
                .execute(&pool)
                .await
                .map_err(|e| e.to_string())?;
            Ok(())
        }
        _ => Err("create_database is SQL-only; redis has no schemas".to_string()),
    }
}

#[tauri::command]
pub async fn drop_database(id: String, name: String) -> Result<(), String> {
    check_db_name(&name)?;
    let inst = crate::state::load_instance(&id)?;
    match inst.engine.as_str() {
        "mysql" | "mariadb" => {
            let pool = sqlx::MySqlPool::connect(&mysql_url("127.0.0.1", inst.port, "mysql"))
                .await
                .map_err(|e| e.to_string())?;
            sqlx::query(&format!("DROP DATABASE `{name}`"))
                .execute(&pool)
                .await
                .map_err(|e| e.to_string())?;
            Ok(())
        }
        "postgres" => {
            let pool = sqlx::PgPool::connect(&pg_url("127.0.0.1", inst.port, "postgres"))
                .await
                .map_err(|e| e.to_string())?;
            sqlx::query(&format!("DROP DATABASE \"{name}\""))
                .execute(&pool)
                .await
                .map_err(|e| e.to_string())?;
            Ok(())
        }
        _ => Err("drop_database is SQL-only; redis has no schemas".to_string()),
    }
}

#[tauri::command]
pub async fn list_schemas(id: String, db: String) -> Result<Vec<String>, String> {
    let inst = crate::state::load_instance(&id)?;
    match inst.engine.as_str() {
        "postgres" => {
            let target = if db.is_empty() { "postgres".to_string() } else { db };
            let pool = sqlx::PgPool::connect(&pg_url("127.0.0.1", inst.port, &target))
                .await
                .map_err(|e| e.to_string())?;
            let rows: Vec<(String,)> = sqlx::query_as(
                "SELECT schema_name FROM information_schema.schemata \
                 WHERE schema_name NOT IN ('pg_catalog', 'information_schema') \
                 AND schema_name NOT LIKE 'pg_toast%' AND schema_name NOT LIKE 'pg_temp%' \
                 ORDER BY 1",
            )
            .fetch_all(&pool)
            .await
            .map_err(|e| e.to_string())?;
            Ok(rows.into_iter().map(|r| r.0).collect())
        }
        other => Err(format!("list_schemas is postgres-only (instance uses {other})")),
    }
}

#[tauri::command]
pub async fn redis_info(id: String) -> Result<String, String> {
    let inst = crate::state::load_instance(&id)?;
    match inst.engine.as_str() {
        "redis" | "valkey" => {
            let client =
                redis::Client::open(redis_url(inst.port)).map_err(|e| e.to_string())?;
            let mut con = client
                .get_multiplexed_async_connection()
                .await
                .map_err(|e| e.to_string())?;
            let pong: String = redis::cmd("PING")
                .query_async(&mut con)
                .await
                .map_err(|e| e.to_string())?;
            let mut lines = vec![format!("ping: {pong}")];
            for i in 0..16 {
                let _: () = redis::cmd("SELECT")
                    .arg(i)
                    .query_async(&mut con)
                    .await
                    .map_err(|e| e.to_string())?;
                let n: usize = redis::cmd("DBSIZE")
                    .query_async(&mut con)
                    .await
                    .map_err(|e| e.to_string())?;
                lines.push(format!("db{i}: {n} keys"));
            }
            Ok(lines.join("\n"))
        }
        other => Err(format!("redis_info is redis-only (instance uses {other})")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mysql_connection_url_has_port_and_db() {
        let url = mysql_url("127.0.0.1", 3307, "mydb");
        assert_eq!(url, "mysql://root:portside@127.0.0.1:3307/mydb");
    }

    #[test]
    fn pg_connection_url_has_port_and_db() {
        let url = pg_url("127.0.0.1", 5433, "mydb");
        assert_eq!(url, "postgres://postgres:portside@127.0.0.1:5433/mydb");
    }

    #[test]
    fn redis_url_has_port_no_auth() {
        let url = redis_url(6380);
        assert_eq!(url, "redis://127.0.0.1:6380/");
    }

    #[test]
    fn db_name_validation_rejects_bad_names() {
        assert!(check_db_name("mydb").is_ok());
        assert!(check_db_name("my_db1").is_ok());
        assert!(check_db_name("").is_err());
        assert!(check_db_name("my-db").is_err());
        assert!(check_db_name("a;b").is_err());
    }
}
