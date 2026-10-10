use clap::Args;

use crate::cli::common::IssueNumber;

#[derive(Debug, Args)]
pub struct LockArgs {
    /// Issue number
    pub number: IssueNumber,
    /// Lock reason
    #[arg(long, value_parser = ["off_topic", "resolved", "spam", "too_heated"], conflicts_with = "undo")]
    pub reason: Option<String>,
    /// Unlock the conversation
    #[arg(long)]
    pub undo: bool,
}
