use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Health {
    pub container_running: bool,
    pub tcp_open: bool,
    pub query_ok: bool,
}

impl Health {
    pub fn is_healthy(&self) -> bool {
        self.container_running && self.tcp_open && self.query_ok
    }
}

#[tauri::command]
pub async fn container_logs(id: String, tail: Option<usize>) -> Result<String, String> {
    let inst = crate::state::load_instance(&id)?;
    let docker = crate::docker::connect()?;
    let opts = bollard::container::LogsOptions::<String> {
        follow: false,
        stdout: true,
        stderr: true,
        since: 0,
        until: 0,
        timestamps: false,
        tail: tail.unwrap_or(200).to_string(),
    };
    use futures_util::TryStreamExt;
    let chunks = docker
        .logs(&inst.container, Some(opts))
        .try_collect::<Vec<_>>()
        .await
        .map_err(|e| format!("logs failed ({}): {e}", inst.container))?;
    Ok(chunks
        .iter()
        .map(|c| c.to_string())
        .collect::<Vec<_>>()
        .join(""))
}

async fn check_query(inst: &crate::docker::Instance) -> bool {
    match inst.engine.as_str() {
        "mysql" | "mariadb" => {
            let Ok(pool) =
                sqlx::MySqlPool::connect(&crate::schemas::mysql_url("127.0.0.1", inst.port, "mysql"))
                    .await
            else {
                return false;
            };
            sqlx::query("SELECT 1").fetch_one(&pool).await.is_ok()
        }
        "postgres" => {
            let Ok(pool) =
                sqlx::PgPool::connect(&crate::schemas::pg_url("127.0.0.1", inst.port, "postgres"))
                    .await
            else {
                return false;
            };
            sqlx::query("SELECT 1").fetch_one(&pool).await.is_ok()
        }
        "redis" | "valkey" => {
            let Ok(client) = redis::Client::open(crate::schemas::redis_url(inst.port)) else {
                return false;
            };
            let Ok(mut con) = client.get_multiplexed_async_connection().await else {
                return false;
            };
            redis::cmd("PING")
                .query_async::<String>(&mut con)
                .await
                .is_ok()
        }
        _ => false,
    }
}

#[tauri::command]
pub async fn health(id: String) -> Result<Health, String> {
    let inst = crate::state::load_instance(&id)?;
    let docker = crate::docker::connect()?;
    let container_running = match docker.inspect_container(&inst.container, None).await {
        Ok(info) => info
            .state
            .as_ref()
            .and_then(|s| s.running)
            .unwrap_or(false),
        Err(_) => false,
    };
    let tcp_open = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        tokio::net::TcpStream::connect(format!("127.0.0.1:{}", inst.port)),
    )
    .await
    .is_ok_and(|r| r.is_ok());
    let query_ok = check_query(&inst).await;
    let h = Health {
        container_running,
        tcp_open,
        query_ok,
    };
    // Canonical healthy/degraded conjunction (single source; also covered by tests).
    let _ = h.is_healthy();
    Ok(h)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn health_ok_is_healthy() {
        assert!(Health { container_running: true, tcp_open: true, query_ok: true }.is_healthy());
    }

    #[test]
    fn health_degraded_is_not_healthy() {
        assert!(!Health { container_running: true, tcp_open: true, query_ok: false }.is_healthy());
        assert!(!Health { container_running: false, tcp_open: true, query_ok: true }.is_healthy());
        assert!(!Health { container_running: true, tcp_open: false, query_ok: true }.is_healthy());
    }
}
