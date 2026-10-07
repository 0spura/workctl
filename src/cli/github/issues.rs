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
    /// Edit issue metadata, relationships, or Project fields
    Edit(EditArgs),
}

#[derive(Debug, Args)]
pub struct CreateArgs {
    /// Issue title; must not be blank
    #[arg(short = 't', long, value_parser = common::parse_non_blank)]
    pub title: String,
    /// Issue body text
    #[arg(short = 'b', long)]
    pub body: Option<String>,
    /// Read the body from a file; `-` reads standard input
    #[arg(short = 'F', long = "body-file", value_name = "FILE")]
    pub body_file: Option<String>,
    /// Add an assignee; may be repeated
    #[arg(long = "assignee", value_name = "LOGIN", value_parser = common::parse_non_blank)]
    pub assignees: Vec<String>,
    /// Add an existing label; use `@auto` to add labels selected by DECISION_MODEL
    #[arg(long = "label", value_name = "NAME", value_parser = common::parse_non_blank)]
    pub labels: Vec<String>,
    /// Set the milestone by name, or `@current` for the nearest open milestone
    /// due today or later
    #[arg(short = 'm', long, value_parser = common::parse_non_blank)]
    pub milestone: Option<String>,
    /// Add to a project; may be repeated
    #[arg(long = "project", value_name = "TITLE", value_parser = common::parse_non_blank)]
    pub projects: Vec<String>,
    /// Set a dynamic field on the configured GitHub Project; repeat as NAME=VALUE
    #[arg(long = "project-field", value_name = "NAME=VALUE", value_parser = common::parse_non_blank)]
    pub project_fields: Vec<String>,
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
    /// Issue numbers or canonical GitHub issue URLs in one repository
    #[arg(required = true, num_args = 1.., value_name = "NUMBER|URL")]
    pub targets: Vec<IssueReference>,
    /// New title; must not be blank
    #[arg(short = 't', long, value_parser = common::parse_non_blank)]
    pub title: Option<String>,
    #[command(flatten)]
    pub change: BodyChangeArgs,
    /// Add an assignee; may be repeated
    #[arg(long = "add-assignee", value_name = "LOGIN", value_delimiter = ',', value_parser = common::parse_non_blank)]
    pub assignees_add: Vec<String>,
    /// Remove an assignee; may be repeated
    #[arg(long = "remove-assignee", value_name = "LOGIN", value_delimiter = ',', value_parser = common::parse_non_blank)]
    pub assignees_remove: Vec<String>,
    /// Add an existing label; use `@auto` to add labels selected by DECISION_MODEL
    #[arg(long = "add-label", value_name = "NAME", value_delimiter = ',', value_parser = common::parse_non_blank)]
    pub labels_add: Vec<String>,
    /// Remove a label; `@auto` is reserved and cannot be removed
    #[arg(long = "remove-label", value_name = "NAME", value_delimiter = ',', value_parser = common::parse_non_blank)]
    pub labels_remove: Vec<String>,
    /// Set the milestone by name, or `@current` for the nearest open milestone
    /// due today or later
    #[arg(short = 'm', long, value_parser = common::parse_non_blank)]
    pub milestone: Option<String>,
    /// Remove the current milestone
    #[arg(long = "remove-milestone", conflicts_with = "milestone")]
    pub clear_milestone: bool,
    /// Add to a project; may be repeated
    #[arg(long = "add-project", value_name = "TITLE", value_delimiter = ',', value_parser = common::parse_non_blank)]
    pub projects_add: Vec<String>,
    /// Remove from a project; may be repeated
    #[arg(long = "remove-project", value_name = "TITLE", value_delimiter = ',', value_parser = common::parse_non_blank)]
    pub projects_remove: Vec<String>,
    /// Set a dynamic field on the configured GitHub Project; repeat as NAME=VALUE
    #[arg(long = "project-field", value_name = "NAME=VALUE", value_parser = common::parse_non_blank)]
    pub project_fields: Vec<String>,
    /// Clear a dynamic field on the configured GitHub Project; may be repeated
    #[arg(long = "clear-project-field", value_name = "NAME", value_parser = common::parse_non_blank)]
    pub clear_project_fields: Vec<String>,
    /// Attach an image or video; may be repeated, optionally as FILE#ALT
    #[arg(long = "attach", value_name = "FILE[#ALT]")]
    pub attachments: Vec<String>,
    /// Refuse the write unless the issue's updated_at matches this value
    #[arg(long = "expect-updated-at", value_name = "TIMESTAMP")]
    pub expect_updated_at: Option<String>,
    /// Set the issue type by name
    #[arg(long = "type", value_name = "NAME", value_parser = common::parse_non_blank, conflicts_with = "remove_type")]
    pub issue_type: Option<String>,
    /// Remove the issue type
    #[arg(long)]
    pub remove_type: bool,
    /// Set the parent issue by number or URL
    #[arg(long, value_name = "NUMBER|URL", conflicts_with = "remove_parent")]
    pub parent: Option<IssueReference>,
    /// Remove the parent issue
    #[arg(long)]
    pub remove_parent: bool,
    /// Add sub-issues by number or URL; repeat or comma-separate
    #[arg(
        long = "add-sub-issue",
        value_name = "NUMBER|URL",
        value_delimiter = ','
    )]
    pub sub_issues_add: Vec<IssueReference>,
    /// Remove sub-issues by number or URL; repeat or comma-separate
    #[arg(
        long = "remove-sub-issue",
        value_name = "NUMBER|URL",
        value_delimiter = ','
    )]
    pub sub_issues_remove: Vec<IssueReference>,
    /// Add issues that block this issue
    #[arg(
        long = "add-blocked-by",
        value_name = "NUMBER|URL",
        value_delimiter = ','
    )]
    pub blocked_by_add: Vec<IssueReference>,
    /// Remove issues that block this issue
    #[arg(
        long = "remove-blocked-by",
        value_name = "NUMBER|URL",
        value_delimiter = ','
    )]
    pub blocked_by_remove: Vec<IssueReference>,
    /// Add issues blocked by this issue
    #[arg(
        long = "add-blocking",
        value_name = "NUMBER|URL",
        value_delimiter = ','
    )]
    pub blocking_add: Vec<IssueReference>,
    /// Remove issues blocked by this issue
    #[arg(
        long = "remove-blocking",
        value_name = "NUMBER|URL",
        value_delimiter = ','
    )]
    pub blocking_remove: Vec<IssueReference>,
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

/// Canonical references are parsed before provider access; no arbitrary URL reaches a mutation.
#[derive(Debug, Clone)]
pub struct IssueReference {
    pub number: u64,
    pub repo: Option<String>,
}

impl std::str::FromStr for IssueReference {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if let Ok(number) = value.parse::<IssueNumber>() {
            return Ok(Self {
                number: number.0,
                repo: None,
            });
        }
        let invalid = "expected a positive issue number or canonical https://github.com/OWNER/REPO/issues/NUMBER URL";
        let path = value.strip_prefix("https://github.com/").ok_or(invalid)?;
        let mut parts = path.split('/');
        let owner = parts.next().ok_or(invalid)?;
        let repo = parts.next().ok_or(invalid)?;
        let kind = parts.next().ok_or(invalid)?;
        let number = parts
            .next()
            .ok_or(invalid)?
            .parse::<IssueNumber>()
            .map_err(|_| invalid)?;
        let valid_part = |part: &str| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
        };
        if kind != "issues" || parts.next().is_some() || !valid_part(owner) || !valid_part(repo) {
            return Err(invalid);
        }
        Ok(Self {
            number: number.0,
            repo: Some(format!("{owner}/{repo}")),
        })
    }
}

impl IssueReference {
    pub fn into_argument(self) -> String {
        match self.repo {
            Some(repo) => format!("https://github.com/{repo}/issues/{}", self.number),
            None => self.number.to_string(),
        }
    }
}
