//! `portside-cli`: headless control for Portside database instances.
//! Same SQLite + Docker state as the desktop app.

mod cmds;
mod output;

use clap::Parser;
use cmds::Cli;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    cmds::run(cli.cmd, cli.json).await;
}
