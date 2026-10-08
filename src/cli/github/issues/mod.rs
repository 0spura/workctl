mod blockers;
mod close;
mod comment;
mod create;
mod edit;
mod list;
mod lock;
mod reopen;
mod references;
mod unlock;
mod view;
pub use blockers::BlockersArgs;

pub use close::CloseArgs;
pub use comment::CommentArgs;
pub use create::CreateArgs;
pub use edit::EditArgs;
pub use list::ListArgs;
pub use lock::LockArgs;
pub use references::IssueReference;
pub use reopen::ReopenArgs;
pub use unlock::UnlockArgs;
pub use view::ViewArgs;

use clap::{Args, Subcommand};

#[derive(Debug, Args)]
#[command(
    disable_help_subcommand = true,
    after_help = "A body edit never requires rewriting the whole issue:\n  \
workctl issue edit 12 --append-body \"Reproduced on 1.4.2.\"\n  \
workctl issue edit 12 --replace-section \"## Acceptance\" --section-body \"New criteria\"\n  \
workctl issue edit 12 --patch-file changes.patch --expect-updated-at 2026-01-02T00:00:00Z\n\n\
See `workctl issue edit --help` for the patch workflow and `workctl issue list --help` for filters."
)]
pub struct IssueArgs {
    #[command(subcommand)]
    pub action: IssueAction,
}

#[derive(Debug, Subcommand)]
pub enum IssueAction {
    /// Create an issue
    Create(CreateArgs),
    /// List issue summaries (never bodies, never pull requests)
    List(ListArgs),
    /// Print only complete chains of open blockers
    Blockers(BlockersArgs),
    /// View one issue with its body
    View(ViewArgs),
    /// Edit issue metadata, relationships, or Project fields
    Edit(EditArgs),
    /// Close an issue
    Close(CloseArgs),
    /// Reopen an issue
    Reopen(ReopenArgs),
    /// Add a comment to an issue
    Comment(CommentArgs),
    /// Lock an issue conversation
    Lock(LockArgs),
    /// Unlock an issue conversation
    Unlock(UnlockArgs),
}
