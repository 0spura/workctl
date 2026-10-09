use clap::Args;

use crate::cli::common::IssueNumber;

#[derive(Debug, Args)]
pub struct StateArgs {
    /// GitLab issue IID
    pub number: IssueNumber,
}
