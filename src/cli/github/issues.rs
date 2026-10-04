use clap::{Args, Subcommand, ValueEnum};

use crate::cli::common::{self, BodyChangeArgs, IssueNumber};

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
    /// View one issue with its body
    View {
        /// Issue number
        number: IssueNumber,
    },
    /// Edit an issue title or body
    Edit(EditArgs),
}

#[derive(Debug, Args)]
pub struct CreateArgs {
    /// Issue title; must not be blank
    #[arg(long, value_parser = common::parse_non_blank)]
    pub title: String,
    /// Issue body text
    #[arg(long)]
    pub body: Option<String>,
    /// Read the body from a file; `-` reads standard input
    #[arg(long = "body-file", value_name = "FILE")]
    pub body_file: Option<String>,
    /// Add an assignee; may be repeated
    #[arg(long = "assignee", value_name = "LOGIN", value_parser = common::parse_non_blank)]
    pub assignees: Vec<String>,
    /// Add an existing label; use `@auto` to add labels selected by DECISION_MODEL
    #[arg(long = "label", value_name = "NAME", value_parser = common::parse_non_blank)]
    pub labels: Vec<String>,
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
    after_help = "Filters pass straight to `gh issue list`. All of them are optional and combine:\n  \
workctl issue list --label bug --label p1 --assignee me --search \"in:title fix\""
)]
pub struct ListArgs {
    /// Issue state to list
    #[arg(long, value_enum, default_value = "open")]
    pub state: ListState,
    /// Maximum number of issues to return
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
    /// Filter by mentioned user login
    #[arg(long, value_parser = common::parse_non_blank)]
    pub mention: Option<String>,
    /// Filter by milestone name
    #[arg(long, value_parser = common::parse_non_blank)]
    pub milestone: Option<String>,
    /// GitHub search query
    #[arg(long, value_parser = common::parse_non_blank)]
    pub search: Option<String>,
    /// Filter by issue type name
    #[arg(long = "type", value_name = "NAME", value_parser = common::parse_non_blank)]
    pub issue_type: Option<String>,
}

#[derive(Debug, Args)]
#[command(
    after_help = "Body changes: pick at most one of --body, --append-body, --replace-section, or\n\
--patch-file. The issue is fetched once, the change is applied to that text, and one write is sent.\n\
With --add-label @auto, the proposed final title/body is sent to DECISION_MODEL; the issue is\n\
fetched again before writing to reject concurrent changes during classification.\n\n\
--patch-file takes standard `git diff` output and locates each hunk by exact context match, not by\n\
line number. Unmatched context is a patch_conflict error and nothing is written; there is no fuzzy\n\
matching. Generate the diff against the body from `workctl issue view <NUMBER>` and pass that\n\
response's updated_at as --expect-updated-at to refuse the write if the issue changed meanwhile."
)]
pub struct EditArgs {
    /// Issue number
    pub number: IssueNumber,
    /// New title; must not be blank
    #[arg(long, value_parser = common::parse_non_blank)]
    pub title: Option<String>,
    #[command(flatten)]
    pub change: BodyChangeArgs,
    /// Add an assignee; may be repeated
    #[arg(long = "add-assignee", value_name = "LOGIN", value_parser = common::parse_non_blank)]
    pub assignees_add: Vec<String>,
    /// Remove an assignee; may be repeated
    #[arg(long = "remove-assignee", value_name = "LOGIN", value_parser = common::parse_non_blank)]
    pub assignees_remove: Vec<String>,
    /// Add an existing label; use `@auto` to add labels selected by DECISION_MODEL
    #[arg(long = "add-label", value_name = "NAME", value_parser = common::parse_non_blank)]
    pub labels_add: Vec<String>,
    /// Remove a label; `@auto` is reserved and cannot be removed
    #[arg(long = "remove-label", value_name = "NAME", value_parser = common::parse_non_blank)]
    pub labels_remove: Vec<String>,
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
    /// Refuse the write unless the issue's updated_at matches this value
    #[arg(long = "expect-updated-at", value_name = "TIMESTAMP")]
    pub expect_updated_at: Option<String>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ListState {
    Open,
    Closed,
    All,
}

impl ListState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Closed => "closed",
            Self::All => "all",
        }
    }
}
