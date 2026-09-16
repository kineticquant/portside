fn userinfo(user: &str, password: &str) -> String {
    if password.is_empty() {
        user.to_string()
    } else {
        format!("{user}:{password}")
    }
}

pub fn mysql_url(host: &str, port: u16, db: &str, user: &str, password: &str, tls: bool) -> String {
    // Passwords are policy-constrained to URL-safe chars, so no escaping needed.
    let ssl = if tls { "?ssl-mode=REQUIRED" } else { "" };
    format!("mysql://{}@{host}:{port}/{db}{ssl}", userinfo(user, password))
}

pub fn pg_url(host: &str, port: u16, db: &str, user: &str, password: &str, tls: bool) -> String {
    let ssl = if tls { "?sslmode=require" } else { "" };
    format!(
        "postgres://{}@{host}:{port}/{db}{ssl}",
        userinfo(user, password)
    )
}

pub fn redis_url(host: &str, port: u16, password: &str, tls: bool) -> String {
    let scheme = if tls { "rediss" } else { "redis" };
    if password.is_empty() {
        format!("{scheme}://{host}:{port}/")
    } else {
        format!("{scheme}://:{password}@{host}:{port}/")
    }
}

/// Redis client honoring password + TLS. LAN instances serve a self-signed
/// cert and default posture is encryption-without-verification (pgAdmin
/// style), so TLS connections skip chain verification.
pub fn redis_client(
    host: &str,
    port: u16,
    password: &str,
    tls: bool,
) -> Result<redis::Client, String> {
    if !tls {
        return redis::Client::open(redis_url(host, port, password, false))
            .map_err(|e| e.to_string());
    }
    let pw = if password.is_empty() {
        None
    } else {
        Some(password.to_string())
    };
    let info = redis::ConnectionInfo {
        addr: redis::ConnectionAddr::TcpTls {
            host: host.to_string(),
            port,
            insecure: true,
            tls_params: None,
        },
        redis: redis::RedisConnectionInfo {
            db: 0,
            username: None,
            password: pw,
            protocol: redis::ProtocolVersion::RESP2,
        },
    };
    redis::Client::open(info).map_err(|e| e.to_string())
}

/// Authenticated multiplexed connection. Password instances require AUTH
/// before any command (including PING) - anonymous PING gets NOAUTH.
pub async fn redis_conn(
    host: &str,
    port: u16,
    password: &str,
    tls: bool,
) -> Result<redis::aio::MultiplexedConnection, String> {
    let client = redis_client(host, port, password, tls)?;
    let mut con = client
        .get_multiplexed_async_connection()
        .await
        .map_err(|e| e.to_string())?;
    if !password.is_empty() {
        redis::cmd("AUTH")
            .arg(password)
            .query_async::<()>(&mut con)
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(con)
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
            let pool = sqlx::MySqlPool::connect(&mysql_url(&inst.host, inst.port, "mysql", inst.db_user_or_default(), &inst.password, inst.tls()))
                .await
                .map_err(|e| e.to_string())?;
            // NOTE: MySQL 8.4 types these name columns VARBINARY, which
            // sqlx cannot decode into String - decode bytes and convert.
            let rows: Vec<(Vec<u8>,)> = sqlx::query_as("SHOW DATABASES")
                .fetch_all(&pool)
                .await
                .map_err(|e| e.to_string())?;
            Ok(rows
                .into_iter()
                .map(|r| String::from_utf8_lossy(&r.0).into_owned())
                .filter(|n| {
                    !["information_schema", "performance_schema", "sys", "mysql"]
                        .contains(&n.as_str())
                })
                .collect())
        }
        "postgres" => {
            let pool = sqlx::PgPool::connect(&pg_url(&inst.host, inst.port, "postgres", inst.db_user_or_default(), &inst.password, inst.tls()))
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
            let mut con = redis_conn(&inst.host, inst.port, &inst.password, inst.tls()).await?;
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
            let pool = sqlx::MySqlPool::connect(&mysql_url(&inst.host, inst.port, "mysql", inst.db_user_or_default(), &inst.password, inst.tls()))
                .await
                .map_err(|e| e.to_string())?;
            sqlx::query(&format!("CREATE DATABASE `{name}`"))
                .execute(&pool)
                .await
                .map_err(|e| e.to_string())?;
            Ok(())
        }
        "postgres" => {
            let pool = sqlx::PgPool::connect(&pg_url(&inst.host, inst.port, "postgres", inst.db_user_or_default(), &inst.password, inst.tls()))
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
            let pool = sqlx::MySqlPool::connect(&mysql_url(&inst.host, inst.port, "mysql", inst.db_user_or_default(), &inst.password, inst.tls()))
                .await
                .map_err(|e| e.to_string())?;
            sqlx::query(&format!("DROP DATABASE `{name}`"))
                .execute(&pool)
                .await
                .map_err(|e| e.to_string())?;
            Ok(())
        }
        "postgres" => {
            let pool = sqlx::PgPool::connect(&pg_url(&inst.host, inst.port, "postgres", inst.db_user_or_default(), &inst.password, inst.tls()))
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
            let pool = sqlx::PgPool::connect(&pg_url(&inst.host, inst.port, &target, inst.db_user_or_default(), &inst.password, inst.tls()))
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
            let mut con = redis_conn(&inst.host, inst.port, &inst.password, inst.tls()).await?;
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
        let url = mysql_url("127.0.0.1", 3307, "mydb", "root", "portside", false);
        assert_eq!(url, "mysql://root:portside@127.0.0.1:3307/mydb");
    }

    #[test]
    fn mysql_tls_url_requires_ssl() {
        let url = mysql_url("192.168.1.10", 3307, "mydb", "root", "s3cret!", true);
        assert_eq!(
            url,
            "mysql://root:s3cret!@192.168.1.10:3307/mydb?ssl-mode=REQUIRED"
        );
    }

    #[test]
    fn pg_connection_url_has_port_and_db() {
        let url = pg_url("127.0.0.1", 5433, "mydb", "postgres", "portside", false);
        assert_eq!(url, "postgres://postgres:portside@127.0.0.1:5433/mydb");
    }

    #[test]
    fn pg_tls_url_requires_ssl() {
        let url = pg_url("192.168.1.10", 5433, "mydb", "postgres", "s3cret!", true);
        assert_eq!(
            url,
            "postgres://postgres:s3cret!@192.168.1.10:5433/mydb?sslmode=require"
        );
    }

    #[test]
    fn pg_url_uses_custom_user_and_host() {
        let url = pg_url("db.lan", 5433, "mydb", "app", "pw", false);
        assert_eq!(url, "postgres://app:pw@db.lan:5433/mydb");
    }

    #[test]
    fn redis_url_has_port_no_auth() {
        let url = redis_url("127.0.0.1", 6380, "", false);
        assert_eq!(url, "redis://127.0.0.1:6380/");
    }

    #[test]
    fn redis_tls_url_uses_rediss_scheme() {
        let url = redis_url("127.0.0.1", 6380, "s3cret!", true);
        assert_eq!(url, "rediss://:s3cret!@127.0.0.1:6380/");
    }

    #[test]
    fn redis_url_uses_given_host() {
        let url = redis_url("db.lan", 6380, "", false);
        assert_eq!(url, "redis://db.lan:6380/");
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
