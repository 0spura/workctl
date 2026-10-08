use clap::Args;

use crate::cli::common::IssueNumber;

#[derive(Debug, Args)]
pub struct BlockersArgs {
    /// Issue whose open blocker chains to show
    pub number: IssueNumber,
}
