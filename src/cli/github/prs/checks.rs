use clap::Args;

use super::shared::PrNumber;

#[derive(Debug, Args)]
#[command(
    after_help = "With --watch, waits for check completion (default limit: 600 seconds, maximum: 3600).\n\
Example:\n  \
workctl pr checks 42 --required --watch --interval 5 --fail-fast"
)]
pub struct ChecksArgs {
    /// Pull request number
    pub number: PrNumber,
    /// Report only the checks the repository requires
    #[arg(long)]
    pub required: bool,
    /// Wait for checks to finish
    #[arg(long)]
    pub watch: bool,
    /// Refresh interval in seconds (1–300; requires --watch)
    #[arg(long, requires = "watch", value_parser = clap::value_parser!(u32).range(1..=300))]
    pub interval: Option<u32>,
    /// Stop watching after the first failed check (requires --watch)
    #[arg(long, requires = "watch")]
    pub fail_fast: bool,
    /// Maximum watch duration in seconds (default 600, range 1–3600)
    #[arg(long = "watch-timeout", requires = "watch", value_parser = clap::value_parser!(u64).range(1..=3600))]
    pub watch_timeout: Option<u64>,
}
