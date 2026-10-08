use clap::Args;

use super::shared::PrNumber;

#[derive(Debug, Args)]
pub struct ReadyArgs {
    /// Pull request number
    pub number: PrNumber,
    /// Convert back to a draft instead
    #[arg(long)]
    pub undo: bool,
}
