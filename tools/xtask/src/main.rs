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

    /// convert trace/v2 firmware logs to a Perfetto trace
    TracePerfetto {
        /// firmware log containing trace/v2 captures
        input: PathBuf,

        /// output Perfetto trace file
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Fixtures => fixtures::generate()?,
        Command::TracePerfetto { input, output } => {
            let output = trace::convert_perfetto(&input, output.as_deref())?;
            println!("wrote {}", output.display(),);
        }
    }

    Ok(())
}
