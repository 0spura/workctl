mod checkout;
mod checks;
mod close;
mod comment;
mod create;
mod diff;
mod edit;
mod list;
mod lock;
mod merge;
mod ready;
mod reopen;
mod review;
mod revert;
mod shared;
mod status;
mod unlock;
mod update_branch;
mod view;

pub use checkout::CheckoutArgs;
pub use checks::ChecksArgs;
pub use close::CloseArgs;
pub use create::CreateArgs;
pub use comment::CommentArgs;
pub use diff::DiffArgs;
pub use edit::EditArgs;
pub use list::ListArgs;
pub use merge::{MergeArgs, MergeMethodArg};
pub use ready::ReadyArgs;
pub use lock::LockArgs;
pub use reopen::ReopenArgs;
pub use review::ReviewArgs;
pub use status::StatusArgs;
pub use update_branch::UpdateBranchArgs;
pub use revert::RevertArgs;
pub use unlock::UnlockArgs;
pub use view::ViewArgs;

use clap::{Args, Subcommand};

#[derive(Debug, Args)]
#[command(
    disable_help_subcommand = true,
    after_help = "Pull request commands follow `gh pr` workflows; status combines review/mergeability evidence with required checks:\n  \
workctl pr list --state open --label bug\n  \
workctl pr status 42\n  \
workctl pr checkout 42\n  \
workctl pr update-branch 42 --rebase\n  \
workctl pr diff 42 --name-only\n  \
workctl pr review 42 --request-changes --body \"Missing test\"\n  \
workctl pr merge 42 --method squash --delete-branch\n\n\
See `workctl pr edit --help` for body changes and `workctl pr status --help` for status limits."
)]
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
    /// Edit a pull request title, body, base, labels, reviewers, or assignees
    Edit(EditArgs),
    /// Mark a pull request ready for review, or back to draft
    Ready(ReadyArgs),
    /// Close a pull request
    Close(CloseArgs),
    /// Reopen a pull request
    Reopen(ReopenArgs),
    /// Add a comment to a pull request
    Comment(CommentArgs),
    /// Lock a pull request conversation
    Lock(LockArgs),
    /// Unlock a pull request conversation
    Unlock(UnlockArgs),
    /// Create a pull request that reverts this pull request
    Revert(RevertArgs),
    /// Update a pull request branch with the latest base branch changes
    UpdateBranch(UpdateBranchArgs),
}
