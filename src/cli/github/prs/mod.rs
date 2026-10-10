mod checkout;
mod checks;
mod comment;
mod create;
mod diff;
mod edit;
mod list;
mod lock;
mod merge;
mod ready;
mod revert;
mod review;
mod shared;
mod status;
mod update_branch;
mod view;

pub use checkout::CheckoutArgs;
pub use checks::ChecksArgs;
pub use comment::CommentArgs;
pub use create::CreateArgs;
pub use diff::DiffArgs;
pub use edit::EditArgs;
pub use list::ListArgs;
pub use lock::LockArgs;
pub use merge::{MergeArgs, MergeMethodArg};
pub use ready::ReadyArgs;
pub use revert::RevertArgs;
pub use review::ReviewArgs;
pub use status::StatusArgs;
pub use update_branch::UpdateBranchArgs;
pub use view::ViewArgs;

use clap::{Args, Subcommand};

#[derive(Debug, Args)]
#[command(disable_help_subcommand = true)]
pub struct PrArgs {
    #[command(subcommand)]
    pub action: PrAction,
}

#[derive(Debug, Subcommand)]
pub enum PrAction {
    /// Open a pull request
    Create(CreateArgs),
    /// Check out a pull request branch in the current worktree
    Checkout(CheckoutArgs),
    /// List pull request summaries (never bodies)
    List(ListArgs),
    /// View one pull request with its body
    View(ViewArgs),
    /// Summarize review, mergeability, and required-check evidence
    Status(StatusArgs),
    /// Print a pull request diff
    Diff(DiffArgs),
    /// Report a pull request's checks
    Checks(ChecksArgs),
    /// Submit a review
    Review(ReviewArgs),
    /// Merge a pull request
    Merge(MergeArgs),
    /// Edit pull request content, metadata, or state
    Edit(EditArgs),
    /// Mark a pull request ready for review, or back to draft
    Ready(ReadyArgs),
    /// Add a comment to a pull request
    Comment(CommentArgs),
    /// Lock or unlock a pull request conversation
    Lock(LockArgs),
    /// Create a pull request that reverts this pull request
    Revert(RevertArgs),
    /// Update a pull request branch with the latest base branch changes
    UpdateBranch(UpdateBranchArgs),
}
