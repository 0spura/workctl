use clap::Args;

use crate::cli::common::{self, IssueNumber};

#[derive(Debug, Args)]
#[command(
    after_help = "Linked branches live on the remote: creating one leaves the current worktree \
untouched until --checkout is given. Without --name, GitHub derives a branch name from the issue."
)]
pub struct DevelopArgs {
    /// Issue number
    pub number: IssueNumber,
    /// Branch to create from; defaults to the repository default branch
    #[arg(long, value_parser = common::parse_non_blank, conflicts_with = "list")]
    pub base: Option<String>,
    /// Name the branch to create
    #[arg(long, value_parser = common::parse_non_blank, conflicts_with = "list")]
    pub name: Option<String>,
    /// Check the created branch out in the current worktree
    #[arg(long, conflicts_with = "list")]
    pub checkout: bool,
    /// List the branches already linked to the issue
    #[arg(long)]
    pub list: bool,
}
