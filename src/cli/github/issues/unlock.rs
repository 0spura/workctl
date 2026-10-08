use clap::Args;

use crate::cli::common::IssueNumber;

#[derive(Debug, Args)]
pub struct UnlockArgs {
    /// Issue number
    pub number: IssueNumber,
}
