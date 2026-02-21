mod cli;
mod client;
mod commands;
mod output;
mod types;

use anyhow::Result;
use clap::Parser;

use cli::Cli;
use client::AionClient;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let client = AionClient::new(&cli.server)?;
    commands::execute(&cli, &client).await
}
