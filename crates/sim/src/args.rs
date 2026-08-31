use std::path::PathBuf;

use clap::Parser;

#[derive(Debug, Parser)]
pub struct SimulatorArgs {
    #[arg(short, long)]
    pub font: Option<PathBuf>,
    #[arg(short, long)]
    pub cover: Option<PathBuf>,
}
