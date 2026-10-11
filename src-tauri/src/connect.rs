//! User-facing connection strings for agents and scripts.
//!
//! Mirrors `connectString` in `src/lib/tauri.ts`: default database names
//! (`postgres` / `mysql`, keyspace `/0` for redis), engine-default users,
//! and the LAN/TLS posture from [`crate::docker::Instance::tls`].

use crate::docker::Instance;

#[derive(Debug, Clone, serde::Serialize)]
pub struct ConnectionInfo {
    pub localhost_url: String,
    pub lan_url: Option<String>,
    pub tls: bool,
}

fn default_password(engine: &str) -> String {
    if engine == "redis" || engine == "valkey" {
        String::new()
    } else {
        "portside".to_string()
    }
}

fn user_for(engine: &str) -> &str {
    if engine == "postgres" {
        "postgres"
    } else {
        "root"
    }
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
            let userinfo = if pw.is_empty() {
                format!("{user}@")
            } else {
                format!("{user}:{pw}@")
            };
            let ssl = if tls {
                if strict {
                    "?sslmode=verify-ca"
                } else {
                    "?sslmode=require"
                }
            } else {
                ""
            };
            format!("postgres://{userinfo}{host}:{}/postgres{ssl}", inst.port)
        }
        "redis" | "valkey" => {
            let scheme = if tls { "rediss" } else { "redis" };
            let auth = if pw.is_empty() {
                String::new()
            } else {
                format!(":{pw}@")
            };
            format!("{scheme}://{auth}{host}:{}/0", inst.port)
        }
        _ => {
            let userinfo = if pw.is_empty() {
                format!("{user}@")
            } else {
                format!("{user}:{pw}@")
            };
            let ssl = if tls {
                if strict {
                    "?ssl-mode=VERIFY_CA"
                } else {
                    "?ssl-mode=REQUIRED"
                }
            } else {
                ""
            };
            format!("mysql://{userinfo}{host}:{}/mysql{ssl}", inst.port)
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
}
