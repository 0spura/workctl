use clap::Args;

use crate::cli::common::IssueNumber;

#[derive(Debug, Args)]
pub struct ReopenArgs {
    /// Issue number
    pub number: IssueNumber,
    /// Leave a reopening comment
    #[arg(long)]
    pub comment: Option<String>,
}
