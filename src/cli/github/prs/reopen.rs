use clap::Args;

use super::shared::PrNumber;

#[derive(Debug, Args)]
pub struct ReopenArgs {
    /// Pull request number
    pub number: PrNumber,
    /// Add a reopening comment
    #[arg(long)]
    pub comment: Option<String>,
}
