use std::path::PathBuf;

use clap::Parser;

#[derive(Debug, Parser)]
pub struct SimulatorArgs {
    #[arg(short, long)]
    pub font: Option<PathBuf>,
    #[arg(short, long, conflicts_with = "epub")]
    pub cover: Option<PathBuf>,
    #[arg(short, long, conflicts_with = "cover")]
    #[cfg_attr(feature = "heap-profile", arg(required = true))]
    pub epub: Option<PathBuf>,
    /// start at the beginning without reading or writing saved progress
    #[arg(long, requires = "epub")]
    pub no_resume: bool,
    /// maximum forward page turns per profiling cycle
    #[cfg(feature = "heap-profile")]
    #[arg(long, default_value_t = 20)]
    pub profile_turns: usize,
    /// number of complete book open/drop cycles
    #[cfg(feature = "heap-profile")]
    #[arg(long, default_value_t = 2, value_parser = clap::value_parser!(u16).range(1..))]
    pub profile_cycles: u16,
    #[cfg(feature = "heap-profile")]
    #[arg(long, default_value = "dhat-heap.json")]
    pub profile_output: PathBuf,
}
