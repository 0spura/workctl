use crate::cli::github::prs::EditArgs;
use crate::cli::GlobalArgs;
use crate::domain::AppError;
use crate::output::SuccessOutput;
use crate::providers::{PrPatch, PullRequestProvider};

use crate::commands::support;

use super::shared;

pub(super) fn execute(globals: &GlobalArgs, args: EditArgs) -> Result<SuccessOutput, AppError> {
    if args.clear_milestone && args.milestone.is_some() {
        return Err(AppError::invalid_input(
            "--milestone and --remove-milestone cannot be used together",
        ));
    }
    let change = support::body_change(&args.change)?;
    let patch = PrPatch {
        title: args.title,
        body: change,
        base: args.base,
        labels_add: args.add_label,
        labels_remove: args.remove_label,
        reviewers_add: args.add_reviewer,
        reviewers_remove: args.remove_reviewer,
        assignees_add: args.add_assignee,
        assignees_remove: args.remove_assignee,
        milestone: args.milestone,
        clear_milestone: args.clear_milestone,
        projects_add: args.projects_add,
        projects_remove: args.projects_remove,
        attachments: args.attachments,
        expect_updated_at: args.expect_updated_at,
    };
    if patch.is_empty() {
        return Err(AppError::invalid_input("update requires a field to change"));
    }
    let provider = shared::provider(globals)?;
    Ok(SuccessOutput::PullRequest(
        provider.edit(args.number.0, &patch)?,
    ))
}
