use std::path::PathBuf;

use clap::Parser;

#[derive(Debug, Parser)]
pub struct SimulatorArgs {
    /// TTF/OTF font used by the mockup.
    #[arg(short, long)]
    pub font: Option<PathBuf>,
}
