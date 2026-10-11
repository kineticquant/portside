//! `portside-cli` command handlers. Read-only commands land first;
//! lifecycle (Task 4) and the MCP server (Task 5) follow.

use crate::output::{docker_hint, fail, print_json, usage};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "portside-cli",
    version,
    about = "Headless control for Portside database instances"
)]
pub struct Cli {
    /// Machine-readable JSON output (for agents and scripts)
    #[arg(long, global = true)]
    pub json: bool,
    #[command(subcommand)]
    pub cmd: Cmd,
}

#[derive(Subcommand)]
pub enum Cmd {
    /// List managed instances
    List,
    /// Show the bundled engine catalog
    Catalog,
    /// Start a stopped instance
    Start { id: String },
    /// Stop a running instance
    Stop { id: String },
    /// Remove container (keep volume) or --wipe (drop volume too)
    Rm {
        id: String,
        #[arg(long)]
        wipe: bool,
    },
    /// Create an instance; --port 0 = auto; waits until healthy then prints connect strings
    Create {
        #[arg(long)]
        engine: String,
        #[arg(long)]
        tag: String,
        #[arg(long, default_value_t = 0)]
        port: u16,
        #[arg(long)]
        lan: bool,
        #[arg(long, default_value = "")]
        password: String,
        #[arg(long, default_value_t = 60)]
        wait: u64,
    },
    /// Databases inside an instance
    Db {
        #[command(subcommand)]
        cmd: DbCmd,
    },
    /// Print connect strings (localhost always; LAN when --lan and a LAN IP is detectable)
    Conn {
        id: String,
        #[arg(long)]
        lan: bool,
        #[arg(long)]
        strict: bool,
    },
    /// Tail container logs
    Logs {
        id: String,
        #[arg(long, default_value_t = 200)]
        tail: usize,
    },
    /// Health check (single shot, or --wait SECONDS polling)
    Health {
        id: String,
        #[arg(long, default_value_t = 0)]
        wait: u64,
    },
    /// MCP server over stdio (for agents)
    Mcp,
}

#[derive(Subcommand)]
pub enum DbCmd {
    List { id: String },
    Create { id: String, name: String },
    Drop { id: String, name: String },
}

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
                    println!(
                        "{}  {}:{}  {}  {}",
                        i.id, i.engine, i.port, i.status, i.container
                    );
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
            let inst =
                portside::state::load_instance(&id).unwrap_or_else(|e| usage(&e));
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
                    h.container_running,
                    h.tcp_open,
                    h.query_ok
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
                if as_json {
                    print_json(&dbs);
                } else if dbs.is_empty() {
                    println!("no databases");
                } else {
                    for d in &dbs {
                        println!("{d}");
                    }
                }
            }
            DbCmd::Create { id, name } => {
                portside::schemas::create_database(id, name)
                    .await
                    .unwrap_or_else(|e| {
                        // Validation errors (bad name) are usage errors, not runtime.
                        if e.contains("must be") || e.contains("SQL-only") {
                            usage(&e);
                        }
                        fail(e, docker_hint())
                    });
                if as_json {
                    print_json(&serde_json::json!({ "ok": true }));
                } else {
                    println!("created");
                }
            }
            DbCmd::Drop { id, name } => {
                portside::schemas::drop_database(id, name)
                    .await
                    .unwrap_or_else(|e| {
                        if e.contains("must be") || e.contains("SQL-only") {
                            usage(&e);
                        }
                        fail(e, docker_hint())
                    });
                if as_json {
                    print_json(&serde_json::json!({ "ok": true }));
                } else {
                    println!("dropped");
                }
            }
        },
        Cmd::Start { id } => {
            portside::docker::start_instance(id)
                .await
                .unwrap_or_else(|e| fail(e, docker_hint()));
            if as_json {
                print_json(&serde_json::json!({ "ok": true }));
            } else {
                println!("started");
            }
        }
        Cmd::Stop { id } => {
            portside::docker::stop_instance(id)
                .await
                .unwrap_or_else(|e| fail(e, docker_hint()));
            if as_json {
                print_json(&serde_json::json!({ "ok": true }));
            } else {
                println!("stopped");
            }
        }
        Cmd::Rm { id, wipe } => {
            portside::docker::remove_instance(id, wipe)
                .await
                .unwrap_or_else(|e| fail(e, docker_hint()));
            if as_json {
                print_json(&serde_json::json!({ "ok": true, "wipe": wipe }));
            } else if wipe {
                println!("removed and wiped");
            } else {
                println!("removed (volume kept)");
            }
        }
        Cmd::Create {
            engine,
            tag,
            port,
            lan,
            password,
            wait,
        } => {
            let (tag, port) = resolve_create(&engine, &tag, port).unwrap_or_else(|e| {
                if e.starts_with("unknown engine") || e == "no free ports" {
                    usage(&e);
                }
                fail(e, docker_hint())
            });
            let bind_ip = if lan { "0.0.0.0" } else { "127.0.0.1" };
            let inst = portside::docker::create_instance(
                engine,
                tag,
                port,
                Some(bind_ip.to_string()),
                Some(password),
            )
            .await
            .unwrap_or_else(|e| fail(e, docker_hint()));
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
                println!(
                    "created {} ({}:{} {})",
                    fresh.id, fresh.engine, fresh.port, fresh.status
                );
                println!("{}", info.localhost_url);
                if let Some(url) = info.lan_url {
                    println!("{url}");
                }
            }
        }
        Cmd::Mcp => usage("mcp server lands in Task 5"),
    }
}

/// Resolve create defaults without touching Docker: empty tag becomes the
/// catalog's first tag for the engine; port 0 becomes the suggested free
/// port given already-saved instances.
pub fn resolve_create(engine: &str, tag: &str, port: u16) -> Result<(String, u16), String> {
    let tag = if tag.is_empty() {
        portside::catalog::bundled_catalog()
            .entries
            .iter()
            .find(|e| e.engine == engine)
            .map(|e| e.tag.clone())
            .ok_or_else(|| format!("unknown engine {engine}"))?
    } else {
        tag.to_string()
    };
    let port = if port == 0 {
        let taken: Vec<u16> = portside::state::list_saved_instances()
            .map(|v| v.iter().map(|i| i.port).collect())
            .unwrap_or_default();
        portside::catalog::suggest_port(engine.to_string(), taken)?
    } else {
        port
    };
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
