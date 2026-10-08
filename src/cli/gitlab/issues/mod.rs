mod create;
mod list;
mod shared;
mod update;
mod view;

pub use create::CreateArgs;
pub use list::ListArgs;
pub use update::UpdateArgs;
pub use view::ViewArgs;

use clap::{Args, Subcommand};

#[derive(Debug, Args)]
#[command(
    disable_help_subcommand = true,
    after_help = "GitLab issue operations mirror `glab issue`:\n  \
workctl issue list --all --label bug\n  \
workctl issue list --assignee @me --search crash\n  \
workctl issue view 12\n\n\
`glab` names the filters differently from `gh`: --closed/--all select the state and --per-page\n\
bounds the result, so `gh`'s --state and --limit are not accepted here."
)]
pub struct IssueArgs {
    #[command(subcommand)]
    pub action: IssueAction,
}

#[derive(Debug, Subcommand)]
pub enum IssueAction {
    /// Create an issue
    Create(CreateArgs),
    /// List issue summaries (never merge requests)
    List(ListArgs),
    /// View one issue with its description
    View(ViewArgs),
    /// Update one issue
    Update(UpdateArgs),
}
