use clap::Args;

use crate::cli::common::{self, BodyChangeArgs, LifecycleState};
use crate::domain::ClosingReference;

use super::shared::PrNumber;

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
    #[arg(long, short = 't', value_parser = common::parse_non_blank)]
    pub title: Option<String>,
    #[command(flatten)]
    pub change: BodyChangeArgs,
    /// Issue closed when the pull request merges, as NUMBER or OWNER/REPO#NUMBER; may be repeated
    #[arg(long = "closes", value_name = "NUMBER|OWNER/REPO#NUMBER")]
    pub closes: Vec<ClosingReference>,
    /// Remove a closing reference from the body, as NUMBER or OWNER/REPO#NUMBER; may be repeated
    #[arg(long = "remove-closes", value_name = "NUMBER|OWNER/REPO#NUMBER")]
    pub remove_closes: Vec<ClosingReference>,
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
    #[arg(long, short = 'm', value_parser = common::parse_non_blank)]
    pub milestone: Option<String>,
    /// Remove the current milestone
    #[arg(long = "remove-milestone")]
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
    /// Set pull-request state after other edits
    #[arg(long, value_enum)]
    pub state: Option<LifecycleState>,
    /// Comment on the state transition
    #[arg(long, value_parser = common::parse_non_blank)]
    pub comment: Option<String>,
    /// Delete the remote branch after closing, keeping the local branch
    #[arg(long = "delete-branch")]
    pub delete_branch: bool,
}
