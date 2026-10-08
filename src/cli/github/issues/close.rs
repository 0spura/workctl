use clap::Args;

use crate::cli::common::IssueNumber;

use super::references::IssueReference;

#[derive(Debug, Args)]
pub struct CloseArgs {
    /// Issue number
    pub number: IssueNumber,
    /// Leave a closing comment
    #[arg(long)]
    pub comment: Option<String>,
    /// Reason the issue is being closed
    #[arg(long, value_parser = ["completed", "not planned", "duplicate"])]
    pub reason: Option<String>,
    /// Mark the issue as a duplicate of this issue number or canonical URL
    #[arg(long = "duplicate-of")]
    pub duplicate_of: Option<IssueReference>,
}
