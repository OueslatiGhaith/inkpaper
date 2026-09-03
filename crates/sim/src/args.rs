use std::path::PathBuf;

use clap::Parser;

#[derive(Debug, Parser)]
pub struct SimulatorArgs {
    #[arg(short, long)]
    pub font: Option<PathBuf>,
    #[arg(short, long, conflicts_with = "epub")]
    pub cover: Option<PathBuf>,
    #[arg(short, long, conflicts_with = "cover")]
    pub epub: Option<PathBuf>,
}
