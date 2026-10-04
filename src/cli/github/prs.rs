use std::str::FromStr;

use clap::{ArgGroup, Args, Subcommand, ValueEnum};

use crate::cli::common::{self, BodyChangeArgs};

#[derive(Debug, Args)]
#[command(
    disable_help_subcommand = true,
    after_help = "Pull request operations mirror `gh pr`:\n  \
workctl pr list --state open --label bug\n  \
workctl pr diff 42 --name-only\n  \
workctl pr review 42 --request-changes --body \"Missing test\"\n  \
workctl pr merge 42 --method squash --delete-branch\n\n\
A body edit never requires rewriting the whole body; see `workctl pr edit --help`."
)]
pub struct PrArgs {
    #[command(subcommand)]
    pub action: PrAction,
}

#[derive(Debug, Subcommand)]
pub enum PrAction {
    /// Open a pull request
    Create(CreateArgs),
    /// List pull request summaries (never bodies)
    List(ListArgs),
    /// View one pull request with its body
    View {
        /// Pull request number
        number: PrNumber,
    },
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
}

#[derive(Debug, Args)]
pub struct CreateArgs {
    /// Pull request title; must not be blank
    #[arg(long, value_parser = common::parse_non_blank)]
    pub title: String,
    /// Pull request body text
    #[arg(long)]
    pub body: Option<String>,
    /// Read the body from a file; `-` reads standard input
    #[arg(long = "body-file", value_name = "FILE")]
    pub body_file: Option<String>,
    /// Branch the pull request merges into; defaults to the repository default branch
    #[arg(long)]
    pub base: Option<String>,
    /// Branch holding the commits; defaults to the current branch
    #[arg(long)]
    pub head: Option<String>,
    /// Open as a draft
    #[arg(long)]
    pub draft: bool,
    /// Issue closed when the pull request merges; may be repeated
    #[arg(long = "closes", value_name = "NUMBER", value_parser = clap::value_parser!(u64).range(1..))]
    pub closes: Vec<u64>,
    /// Add an assignee; may be repeated
    #[arg(long = "assignee", value_name = "LOGIN", value_parser = common::parse_non_blank)]
    pub assignees: Vec<String>,
    /// Add a label; may be repeated
    #[arg(long = "label", value_name = "NAME", value_parser = common::parse_non_blank)]
    pub labels: Vec<String>,
    /// Request a review from a login; may be repeated
    #[arg(long = "reviewer", value_name = "LOGIN", value_parser = common::parse_non_blank)]
    pub reviewers: Vec<String>,
    /// Set the milestone by name, or `@current` for the nearest open milestone
    /// due today or later
    #[arg(long, value_parser = common::parse_non_blank)]
    pub milestone: Option<String>,
    /// Add to a project; may be repeated
    #[arg(long = "project", value_name = "TITLE", value_parser = common::parse_non_blank)]
    pub projects: Vec<String>,
    /// Attach an image or video; may be repeated, optionally as FILE#ALT
    #[arg(long = "attach", value_name = "FILE[#ALT]")]
    pub attachments: Vec<String>,
}

#[derive(Debug, Args)]
#[command(
    after_help = "Filters pass straight to `gh pr list`. All of them are optional and combine:\n  \
workctl pr list --state merged --author me --search \"in:title workctl\""
)]
pub struct ListArgs {
    /// Pull request state to list
    #[arg(long, value_enum, default_value = "open")]
    pub state: PrListState,
    /// Maximum number of pull requests to return
    #[arg(long, default_value_t = 30, value_parser = common::parse_limit)]
    pub limit: usize,
    /// Filter by label; may be repeated
    #[arg(long = "label", value_name = "NAME", value_parser = common::parse_non_blank)]
    pub labels: Vec<String>,
    /// Filter by assignee login
    #[arg(long, value_parser = common::parse_non_blank)]
    pub assignee: Option<String>,
    /// Filter by author login
    #[arg(long, value_parser = common::parse_non_blank)]
    pub author: Option<String>,
    /// Filter by base branch
    #[arg(long, value_parser = common::parse_non_blank)]
    pub base: Option<String>,
    /// Filter by head branch
    #[arg(long, value_parser = common::parse_non_blank)]
    pub head: Option<String>,
    /// GitHub search query
    #[arg(long, value_parser = common::parse_non_blank)]
    pub search: Option<String>,
    /// Filter by draft state
    #[arg(long)]
    pub draft: bool,
}

#[derive(Debug, Args)]
pub struct DiffArgs {
    /// Pull request number
    pub number: PrNumber,
    /// Print only the names of changed files
    #[arg(long = "name-only")]
    pub name_only: bool,
}

#[derive(Debug, Args)]
pub struct ChecksArgs {
    /// Pull request number
    pub number: PrNumber,
    /// Report only the checks the repository requires
    #[arg(long)]
    pub required: bool,
}

#[derive(Debug, Args)]
#[command(group = ArgGroup::new("event").required(true).args(["approve", "request_changes", "comment"]))]
pub struct ReviewArgs {
    /// Pull request number
    pub number: PrNumber,
    /// Approve the pull request
    #[arg(long)]
    pub approve: bool,
    /// Request changes
    #[arg(long = "request-changes")]
    pub request_changes: bool,
    /// Leave a review comment without approving or requesting changes
    #[arg(long)]
    pub comment: bool,
    /// Review body text
    #[arg(long)]
    pub body: Option<String>,
    /// Read the review body from a file; `-` reads standard input
    #[arg(long = "body-file", value_name = "FILE")]
    pub body_file: Option<String>,
}

#[derive(Debug, Args)]
#[command(
    after_help = "`gh pr merge` needs a merge method, and workctl passes none unless you give\n\
--method, so an explicit method is required whenever gh cannot infer one."
)]
pub struct MergeArgs {
    /// Pull request number
    pub number: PrNumber,
    /// Merge method
    #[arg(long, value_enum)]
    pub method: Option<MergeMethodArg>,
    /// Delete the local and remote branch after merge
    #[arg(long = "delete-branch")]
    pub delete_branch: bool,
    /// Queue the merge once the repository requirements are met
    #[arg(long)]
    pub auto: bool,
}

#[derive(Debug, Args)]
#[command(
    after_help = "Body changes: pick at most one of --body, --append-body, --replace-section, or\n\
--patch-file. The pull request is fetched once, the change is applied to that text, and one write\n\
is sent. Pass the updated_at from `workctl pr view <NUMBER>` as --expect-updated-at to refuse the\n\
write if the pull request changed meanwhile."
)]
pub struct EditArgs {
    /// Pull request number
    pub number: PrNumber,
    /// New title; must not be blank
    #[arg(long, value_parser = common::parse_non_blank)]
    pub title: Option<String>,
    #[command(flatten)]
    pub change: BodyChangeArgs,
    /// Change the base branch
    #[arg(long)]
    pub base: Option<String>,
    /// Add a label; may be repeated
    #[arg(long = "add-label", value_name = "NAME", value_parser = common::parse_non_blank)]
    pub add_label: Vec<String>,
    /// Remove a label; may be repeated
    #[arg(long = "remove-label", value_name = "NAME", value_parser = common::parse_non_blank)]
    pub remove_label: Vec<String>,
    /// Request a review from a login; may be repeated
    #[arg(long = "add-reviewer", value_name = "LOGIN", value_parser = common::parse_non_blank)]
    pub add_reviewer: Vec<String>,
    /// Remove a review request; may be repeated
    #[arg(long = "remove-reviewer", value_name = "LOGIN", value_parser = common::parse_non_blank)]
    pub remove_reviewer: Vec<String>,
    /// Add an assignee; may be repeated
    #[arg(long = "add-assignee", value_name = "LOGIN", value_parser = common::parse_non_blank)]
    pub add_assignee: Vec<String>,
    /// Remove an assignee; may be repeated
    #[arg(long = "remove-assignee", value_name = "LOGIN", value_parser = common::parse_non_blank)]
    pub remove_assignee: Vec<String>,
    /// Set the milestone by name, or `@current` for the nearest open milestone
    /// due today or later
    #[arg(long, value_parser = common::parse_non_blank)]
    pub milestone: Option<String>,
    /// Remove the current milestone
    #[arg(long)]
    pub clear_milestone: bool,
    /// Add to a project; may be repeated
    #[arg(long = "add-project", value_name = "TITLE", value_parser = common::parse_non_blank)]
    pub projects_add: Vec<String>,
    /// Remove from a project; may be repeated
    #[arg(long = "remove-project", value_name = "TITLE", value_parser = common::parse_non_blank)]
    pub projects_remove: Vec<String>,
    /// Attach an image or video; may be repeated, optionally as FILE#ALT
    #[arg(long = "attach", value_name = "FILE[#ALT]")]
    pub attachments: Vec<String>,
    /// Refuse the write unless the pull request's updated_at matches this value
    #[arg(long = "expect-updated-at", value_name = "TIMESTAMP")]
    pub expect_updated_at: Option<String>,
}

#[derive(Debug, Args)]
pub struct ReadyArgs {
    /// Pull request number
    pub number: PrNumber,
    /// Convert back to a draft instead
    #[arg(long)]
    pub undo: bool,
}

#[derive(Debug, Args)]
pub struct CloseArgs {
    /// Pull request number
    pub number: PrNumber,
    /// Leave a closing comment
    #[arg(long)]
    pub comment: Option<String>,
    /// Delete the local and remote branch after closing
    #[arg(long = "delete-branch")]
    pub delete_branch: bool,
}

#[derive(Debug, Args)]
pub struct ReopenArgs {
    /// Pull request number
    pub number: PrNumber,
    /// Add a reopening comment
    #[arg(long)]
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum PrListState {
    Open,
    Closed,
    Merged,
    All,
}

impl PrListState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Closed => "closed",
            Self::Merged => "merged",
            Self::All => "all",
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum MergeMethodArg {
    Merge,
    Squash,
    Rebase,
}

impl MergeMethodArg {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Merge => "merge",
            Self::Squash => "squash",
            Self::Rebase => "rebase",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PrNumber(pub u64);

impl FromStr for PrNumber {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let number = value
            .parse::<u64>()
            .map_err(|_| "expected a positive pull request number")?;
        if number == 0 {
            return Err("expected a positive pull request number");
        }
        Ok(Self(number))
    }
}
