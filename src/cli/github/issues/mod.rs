mod blockers;
mod comment;
mod create;
mod develop;
mod edit;
mod list;
mod lock;
mod references;
mod view;

pub use blockers::BlockersArgs;
pub use comment::CommentArgs;
pub use create::CreateArgs;
pub use develop::DevelopArgs;
pub use edit::EditArgs;
pub use list::ListArgs;
pub use lock::LockArgs;
pub use references::IssueReference;
pub use view::ViewArgs;

use clap::{Args, Subcommand};

#[derive(Debug, Args)]
#[command(disable_help_subcommand = true)]
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
    /// Edit issue content, metadata, relationships, or Project fields
    Edit(EditArgs),
    /// Add a comment to an issue
    Comment(CommentArgs),
    /// Lock an issue conversation
    Lock(LockArgs),
    /// Create or list the branches linked to an issue
    Develop(DevelopArgs),
}
