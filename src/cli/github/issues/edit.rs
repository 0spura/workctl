use clap::Args;

use crate::cli::common::{self, BodyChangeArgs};

use super::references::IssueReference;

#[derive(Debug, Args)]
#[command(
    after_help = "Body changes: pick at most one of --body, --append-body, --replace-section, or\n\
--patch-file. The issue is fetched once, the change is applied to that text, and one write is sent.\n\
With --add-label @auto or --project-field 'Priority=@auto', the final title/body is sent to\n\
DECISION_MODEL in one request; the issue is fetched again before writes to reject concurrent\n\
changes. Only named single-select/iteration fields are selected; scores below 0.8 or tied best\n\
options preserve the current field value. Creation defaults are not reapplied.\n\n\
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
    /// Set a Project field as NAME=VALUE; NAME=@auto selects a model-chosen option
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
