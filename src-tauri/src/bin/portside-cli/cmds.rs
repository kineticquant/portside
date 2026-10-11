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
            _ => usage("db create/drop land in Task 4"),
        },
        Cmd::Start { .. } | Cmd::Stop { .. } | Cmd::Rm { .. } | Cmd::Create { .. } => {
            usage("lifecycle commands land in Task 4")
        }
        Cmd::Mcp => usage("mcp server lands in Task 5"),
    }
}
