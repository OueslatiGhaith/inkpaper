use std::path::PathBuf;

use clap::{Parser, Subcommand};

mod fixtures;
mod perf;
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

    /// inspect InkPaper firmware performance logs
    Perf {
        #[command(subcommand)]
        command: PerfCommand,
    },

    /// convert trace/v2 firmware logs to a Perfetto trace
    TracePerfetto {
        /// firmware log containing trace/v2 captures
        input: PathBuf,

        /// output Perfetto trace file
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

#[derive(Debug, Subcommand)]
enum PerfCommand {
    /// summarize a firmware performance capture
    Summary { input: PathBuf },
    /// compare two firmware performance captures
    Compare { before: PathBuf, after: PathBuf },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Fixtures => fixtures::generate()?,
        Command::Perf { command } => match command {
            PerfCommand::Summary { input } => perf::summary(&input)?,
            PerfCommand::Compare { before, after } => perf::compare(&before, &after)?,
        },
        Command::TracePerfetto { input, output } => {
            let output = trace::convert_perfetto(&input, output.as_deref())?;
            println!("wrote {}", output.display(),);
        }
    }

    Ok(())
}
