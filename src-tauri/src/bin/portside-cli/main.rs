//! `portside-cli`: headless control for Portside database instances.
//! Same SQLite + Docker state as the desktop app.

mod cmds;
mod mcp;
mod output;

use clap::Parser;
use cmds::Cli;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    if matches!(cli.cmd, cmds::Cmd::Mcp) {
        mcp::serve().await;
        return;
    }
    cmds::run(cli.cmd, cli.json).await;
}
