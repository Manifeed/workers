mod cli;
mod commands;

use clap::Parser;

use cli::Cli;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    tracing_subscriber::fmt().with_target(false).init();

    commands::dispatch(cli.command.unwrap_or_else(commands::default_command)).await
}
