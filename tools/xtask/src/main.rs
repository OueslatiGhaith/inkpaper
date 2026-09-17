use clap::{Parser, Subcommand};

mod fixtures;

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// generate EPUB fixtures
    Fixtures,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Fixtures => fixtures::generate(),
    }
}
