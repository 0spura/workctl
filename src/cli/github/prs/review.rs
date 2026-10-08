use clap::{ArgGroup, Args};

use super::shared::PrNumber;

#[derive(Debug, Args)]
#[command(group = ArgGroup::new("event").required(true).args(["approve", "request_changes", "comment"]))]
pub struct ReviewArgs {
    /// Pull request number
    pub number: PrNumber,
    /// Approve the pull request
    #[arg(long)]
    pub approve: bool,
    /// Request changes
    #[arg(long = "request-changes")]
    pub request_changes: bool,
    /// Leave a review comment without approving or requesting changes
    #[arg(long)]
    pub comment: bool,
    /// Review body text
    #[arg(long, short = 'b')]
    pub body: Option<String>,
    /// Read the review body from a file; `-` reads standard input
    #[arg(long = "body-file", short = 'F', value_name = "FILE")]
    pub body_file: Option<String>,
}
