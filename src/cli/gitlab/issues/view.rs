use clap::Args;

use crate::cli::common::IssueNumber;

#[derive(Debug, Args)]
pub struct ViewArgs {
    /// Issue number (GitLab IID)
    pub number: IssueNumber,
}
