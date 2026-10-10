use clap::Args;

use super::shared::PrNumber;

#[derive(Debug, Args)]
pub struct LockArgs {
    /// Pull request number
    pub number: PrNumber,
    /// Lock reason
    #[arg(long, value_parser = ["off_topic", "resolved", "spam", "too_heated"], conflicts_with = "undo")]
    pub reason: Option<String>,
    /// Unlock the conversation
    #[arg(long)]
    pub undo: bool,
}
