//! Live Docker E2E (LAN postgres + LAN redis through the real commands).
//!
//! Ignored by default: needs a running Docker daemon and pulls real images.
//! Run: `cargo test --manifest-path src-tauri/Cargo.toml -- --ignored e2e`
//! Uses high test-only ports and always wipes its instances, even on failure.

#![cfg(test)]

use crate::{catalog, docker, health, schemas, state};

// NOTE: keep clear of Hyper-V/WSL excluded TCP ranges on Windows
// (e.g. 50000-50059, 52899-53498 here) or binds fail with EACCES.
const PG_PORT: u16 = 55433;
const REDIS_PORT: u16 = 56380;
const MYSQL_PORT: u16 = 13307;

async fn wait_healthy(id: &str) -> bool {
    for _ in 0..30 {
        if let Ok(h) = health::health(id.to_string()).await {
            if h.container_running && h.tcp_open && h.query_ok {
                return true;
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    }
    false
}

async fn cleanup(id: &str) {
    let _ = docker::remove_instance(id.to_string(), true).await;
}

/// Last container logs, appended to failure messages so a failed run
/// diagnoses itself instead of just saying "never became healthy".
async fn tail(id: &str) -> String {
    health::container_logs(id.to_string(), Some(40))
        .await
        .unwrap_or_else(|e| format!("<no logs: {e}>"))
}

#[tokio::test]
#[ignore]
async fn e2e_lan_postgres() -> Result<(), String> {
    let inst = docker::create_instance(
        "postgres".to_string(),
        "17".to_string(),
        PG_PORT,
        Some("0.0.0.0".to_string()),
        Some("s3cret!".to_string()),
    )
    .await?;
    let id = inst.id.clone();
    assert!(inst.tls(), "LAN instance must serve TLS");

    let outcome: Result<(), String> = async {
        if !wait_healthy(&id).await {
            return Err(format!(
                "postgres LAN instance never became healthy. logs:\n{}",
                tail(&id).await
            ));
        }
        let dbs = schemas::list_databases(id.clone()).await?;
        if !dbs.iter().any(|d| d == "postgres") {
            return Err(format!("expected postgres db, got {dbs:?}"));
        }
        schemas::create_database(id.clone(), "e2e_db1".to_string()).await?;
        let dbs = schemas::list_databases(id.clone()).await?;
        if !dbs.iter().any(|d| d == "e2e_db1") {
            return Err("e2e_db1 missing after create".to_string());
        }
        schemas::drop_database(id.clone(), "e2e_db1".to_string()).await?;
        let pem = crate::certs::read_cert_pem(&id)?;
        if !pem.contains("BEGIN CERTIFICATE") {
            return Err("server cert is not PEM".to_string());
        }
        Ok(())
    }
    .await;

    cleanup(&id).await;
    // Instance row must be gone after wipe.
    let remaining = state::list_saved_instances()?;
    assert!(
        !remaining.iter().any(|i| i.id == id),
        "wiped instance row still present"
    );
    outcome
}

#[tokio::test]
#[ignore]
async fn e2e_lan_mysql() -> Result<(), String> {
    let inst = docker::create_instance(
        "mysql".to_string(),
        "8.4".to_string(),
        MYSQL_PORT,
        Some("0.0.0.0".to_string()),
        Some("s3cret!".to_string()),
    )
    .await?;
    let id = inst.id.clone();
    assert!(inst.tls(), "LAN instance must serve TLS");

    let outcome: Result<(), String> = async {
        if !wait_healthy(&id).await {
            return Err(format!(
                "mysql LAN instance never became healthy. logs:\n{}",
                tail(&id).await
            ));
        }
        schemas::create_database(id.clone(), "e2e_db1".to_string()).await?;
        let dbs = schemas::list_databases(id.clone()).await?;
        if !dbs.iter().any(|d| d == "e2e_db1") {
            return Err("e2e_db1 missing after create".to_string());
        }
        schemas::drop_database(id.clone(), "e2e_db1".to_string()).await?;
        Ok(())
    }
    .await;

    cleanup(&id).await;
    outcome
}

#[tokio::test]
#[ignore]
async fn e2e_lan_redis() -> Result<(), String> {
    let _ = catalog::get_catalog();
    let inst = docker::create_instance(
        "redis".to_string(),
        "8".to_string(),
        REDIS_PORT,
        Some("0.0.0.0".to_string()),
        Some("s3cret!".to_string()),
    )
    .await?;
    let id = inst.id.clone();
    assert!(inst.tls(), "LAN instance must serve TLS");

    let outcome: Result<(), String> = async {
        if !wait_healthy(&id).await {
            return Err(format!(
                "redis LAN instance never became healthy. logs:\n{}",
                tail(&id).await
            ));
        }
        let info = schemas::redis_info(id.clone()).await?;
        if !info.contains("ping: PONG") {
            return Err(format!("unexpected redis info: {info}"));
        }
        Ok(())
    }
    .await;

    cleanup(&id).await;
    outcome
}
