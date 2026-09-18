use std::path::PathBuf;

use clap::{Parser, Subcommand};

mod fixtures;
mod trace;

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// generate EPUB fixtures
    Fixtures,
    /// convert inkpaper firmware trace logs to Speedscope json
    TraceSpeedscope {
        /// firmware log containing trace/session and trace/span records
        input: PathBuf,
        /// output Speedscope json file
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Fixtures => fixtures::generate()?,
        Command::TraceSpeedscope { input, output } => {
            let output = trace::convert(&input, output.as_deref())?;
            println!("wrote {}", output.display());
        }
    }

    Ok(())
}
