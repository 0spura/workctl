use clap::Args;

use super::shared::PrNumber;

#[derive(Debug, Args)]
#[command(
    after_help = "Updates the pull request branch with the latest base branch changes. Default is a merge commit; --rebase rebases instead.\n\nExample:\n  workctl pr update-branch 42 --rebase"
)]
pub struct UpdateBranchArgs {
    /// Pull request number
    pub number: PrNumber,
    /// Rebase the pull request branch onto the latest base branch
    #[arg(long)]
    pub rebase: bool,
}
