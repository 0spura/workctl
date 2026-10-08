use clap::Args;

use super::shared::PrNumber;

#[derive(Debug, Args)]
#[command(
    after_help = "Checks out the pull-request branch in the current worktree. Existing local changes are protected by GitHub CLI's normal checkout behavior; workctl never passes --force.\n\nExample:\n  workctl pr checkout 42"
)]
pub struct CheckoutArgs {
    /// Pull request number
    pub number: PrNumber,
}
