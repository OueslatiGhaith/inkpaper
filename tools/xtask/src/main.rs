use std::path::PathBuf;

use clap::{Parser, Subcommand};

mod fixtures;
mod text_quality;
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

    /// inspect performance data from trace/v4 firmware logs
    Perf {
        #[command(subcommand)]
        command: PerfCommand,
    },

    /// measure reader text as the device draws it
    TextQuality {
        /// name of this run, written to target/text-quality/<label>
        #[arg(short, long, default_value = "latest")]
        label: String,

        /// label of an earlier run to compare against
        #[arg(short, long)]
        compare: Option<String>,

        /// least glyph coverage (0-255) that inks a pixel, instead of the device default
        #[arg(long)]
        min_ink_coverage: Option<u8>,

        /// draw autohinted glyphs, as firmware built with the `hinting` feature does
        #[arg(long)]
        hinted: bool,
    },

    /// convert trace/v4 firmware logs to a Perfetto trace
    TracePerfetto {
        /// firmware log containing trace/v4 captures
        input: PathBuf,

        /// output Perfetto trace file
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

#[derive(Debug, Subcommand)]
enum PerfCommand {
    /// summarize performance data from a trace/v4 capture
    Summary { input: PathBuf },
    /// compare performance data from two trace/v4 captures
    Compare { before: PathBuf, after: PathBuf },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Fixtures => fixtures::generate()?,
        Command::Perf { command } => match command {
            PerfCommand::Summary { input } => trace::summarize_performance(&input)?,
            PerfCommand::Compare { before, after } => trace::compare_performance(&before, &after)?,
        },
        Command::TextQuality {
            label,
            compare,
            min_ink_coverage,
            hinted,
        } => text_quality::run(&label, compare.as_deref(), min_ink_coverage, hinted)?,
        Command::TracePerfetto { input, output } => {
            let output = trace::convert_perfetto(&input, output.as_deref())?;
            println!("wrote {}", output.display(),);
        }
    }

    Ok(())
}
