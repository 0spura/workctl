use crate::cli::GlobalArgs;
use crate::cli::github::prs::EditArgs;
use crate::domain::AppError;
use crate::output::SuccessOutput;
use crate::providers::{PrPatch, PullRequestProvider};

use crate::commands::support;

use super::shared;

pub(super) fn execute(globals: &GlobalArgs, args: EditArgs) -> Result<SuccessOutput, AppError> {
    if args.comment.is_some() && args.state.is_none() {
        return Err(AppError::invalid_input("--comment requires --state"));
    }
    if args.delete_branch && args.state != Some(crate::cli::common::LifecycleState::Closed) {
        return Err(AppError::invalid_input(
            "--delete-branch requires --state closed",
        ));
    }
    if args.clear_milestone && args.milestone.is_some() {
        return Err(AppError::invalid_input(
            "--milestone and --remove-milestone cannot be used together",
        ));
    }
    let change = support::body_change(&args.change)?;
    if args
        .closes
        .iter()
        .any(|reference| args.remove_closes.contains(reference))
    {
        return Err(AppError::invalid_input(
            "the same closing reference cannot be added and removed",
        ));
    }
    let related_issue_references = args.closes.clone();
    let removed_related_issue_references = args.remove_closes.clone();
    let patch = PrPatch {
        title: args.title,
        body: change,
        closes_add: args.closes,
        closes_remove: args.remove_closes,
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
    if patch.is_empty() && args.state.is_none() {
        return Err(AppError::invalid_input("update requires a field to change"));
    }
    let transition = args.state.map(|state| crate::providers::PrTransition {
        closed: state.is_closed(),
        comment: args.comment,
        delete_branch: args.delete_branch,
    });
    let provider = shared::provider(globals)?;
    let action = match transition.as_ref() {
        Some(transition) if transition.closed => crate::output::MutationAction::Closed,
        Some(_) => crate::output::MutationAction::Reopened,
        None => crate::output::MutationAction::Updated,
    };
    let (pull_request, unchanged) = match provider.edit(args.number.0, &patch, transition.as_ref())
    {
        Ok(result) => result,
        Err(mut error) => {
            if error.code == "partial_success"
                && error.details.as_ref().is_some_and(|details| {
                    details
                        .get("completed")
                        .and_then(serde_json::Value::as_array)
                        .is_some_and(|completed| {
                            completed
                                .iter()
                                .any(|step| step.as_str() == Some("pull request edit"))
                        })
                })
            {
                if let Some(details) = error
                    .details
                    .as_mut()
                    .and_then(serde_json::Value::as_object_mut)
                {
                    details.insert(
                        "relatedIssueReferences".to_owned(),
                        serde_json::json!(
                            related_issue_references
                                .iter()
                                .map(ToString::to_string)
                                .collect::<Vec<_>>()
                        ),
                    );
                    details.insert(
                        "removedRelatedIssueReferences".to_owned(),
                        serde_json::json!(
                            removed_related_issue_references
                                .iter()
                                .map(ToString::to_string)
                                .collect::<Vec<_>>()
                        ),
                    );
                    error.partial_event = Some(crate::domain::PartialEvent {
                        resource: "pull_request",
                        number: args.number.0,
                        operation: "updated",
                    });
                }
            }
            return Err(error);
        }
    };
    Ok(SuccessOutput::PullRequestMutation(
        crate::output::PullRequestMutation {
            pull_request,
            action,
            unchanged,
            gitlab: false,
            related_issue_references,
            removed_related_issue_references,
        },
    ))
}
