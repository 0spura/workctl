use clap::{Args, ValueEnum};

use super::shared::PrNumber;

#[derive(Debug, Args)]
#[command(
    after_help = "Precedence: --method, then defaults.github.pr.mergeMethod, then gh's choice.\n\
`defaults.github.pr.deleteBranch` defaults to false; --delete-branch enables deletion for this merge.\n\
Deletion removes the remote branch only, never a local branch. A queued merge (--auto) deletes\n\
nothing, and pairing it with an explicit --delete-branch is refused."
)]
pub struct MergeArgs {
    /// Pull request number
    pub number: PrNumber,
    /// Merge method
    #[arg(long, value_enum)]
    pub method: Option<MergeMethodArg>,
    /// Delete the remote branch after merge, keeping the local branch (also enabled by config)
    #[arg(long = "delete-branch")]
    pub delete_branch: bool,
    /// Queue the merge once the repository requirements are met
    #[arg(long)]
    pub auto: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum MergeMethodArg {
    Merge,
    Squash,
    Rebase,
}
