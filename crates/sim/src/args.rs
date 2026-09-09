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
    /// start at the beginning without reading or writing saved progress
    #[arg(long, requires = "epub")]
    pub no_resume: bool,
}
