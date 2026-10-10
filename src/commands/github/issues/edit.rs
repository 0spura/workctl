use crate::cli::GlobalArgs;
use crate::cli::github::issues::{EditArgs, IssueReference};
use crate::domain::{AppError, DecisionCandidate};
use crate::output::SuccessOutput;
use crate::providers::github::issues::{GitHubIssues, IssueTransition, NativeIssueEdit};
use crate::providers::{IssuePatch, WorkItemProvider, resolve_body_change};

use crate::commands::support;

use super::shared::filter_label_catalog;
use crate::commands::labels::{AUTO_LABEL_THRESHOLD, automatic_label_names};

pub(super) fn execute(globals: &GlobalArgs, args: EditArgs) -> Result<SuccessOutput, AppError> {
    if args.comment.is_some() && args.state.is_none() {
        return Err(AppError::invalid_input("--comment requires --state"));
    }
    if (args.reason.is_some() || args.duplicate_of.is_some())
        && args.state != Some(crate::cli::common::LifecycleState::Closed)
    {
        return Err(AppError::invalid_input(
            "--reason and --duplicate-of require --state closed",
        ));
    }
    let url_repo = args
        .targets
        .iter()
        .find_map(|target| target.repo.as_deref());
    let provider_hint = globals
        .work_item_provider
        .or_else(|| url_repo.map(|_| crate::config::Provider::Github));
    let repo = support::resolve_repo(
        provider_hint.or(globals.work_item_provider),
        globals.repo.as_deref().or(url_repo),
    )?;
    let mut numbers = Vec::with_capacity(args.targets.len());
    for target in &args.targets {
        if target
            .repo
            .as_ref()
            .is_some_and(|value| !value.eq_ignore_ascii_case(&repo))
        {
            return Err(AppError::invalid_input(
                "all edit targets must belong to the selected repository",
            ));
        }
        if !numbers.contains(&target.number) {
            numbers.push(target.number);
        }
    }
    let transition = args.state.map(|state| IssueTransition {
        closed: state.is_closed(),
        comment: args.comment,
        reason: args.reason,
        duplicate_of: args.duplicate_of.map(IssueReference::into_argument),
    });
    let native = NativeIssueEdit {
        issue_type: args.issue_type,
        remove_type: args.remove_type,
        parent: args.parent.map(IssueReference::into_argument),
        remove_parent: args.remove_parent,
        sub_issues_add: relation_arguments(args.sub_issues_add, &repo),
        sub_issues_remove: relation_arguments(args.sub_issues_remove, &repo),
        blocked_by_add: relation_arguments(args.blocked_by_add, &repo),
        blocked_by_remove: relation_arguments(args.blocked_by_remove, &repo),
        blocking_add: relation_arguments(args.blocking_add, &repo),
        blocking_remove: relation_arguments(args.blocking_remove, &repo),
    };
    for (add, remove) in [
        (&native.sub_issues_add, &native.sub_issues_remove),
        (&native.blocked_by_add, &native.blocked_by_remove),
        (&native.blocking_add, &native.blocking_remove),
    ] {
        if add.iter().any(|value| remove.contains(value)) {
            return Err(AppError::invalid_input(
                "the same relationship cannot be added and removed",
            ));
        }
    }
    if args.labels_remove.iter().any(|label| label == "@auto") {
        return Err(AppError::invalid_input("@auto cannot be removed"));
    }
    let change = support::body_change(&args.change)?;
    let automatic = args.labels_add.iter().any(|label| label == "@auto");
    let labels_add = args
        .labels_add
        .into_iter()
        .filter(|label| label != "@auto")
        .collect::<Vec<_>>();
    let (fields, clears) = project_field_edit(&args.project_fields, &args.clear_project_fields)?;
    let project_edit = !fields.is_empty() || !clears.is_empty();
    let automatic_fields = fields.iter().any(|(_, value)| value == "@auto");
    if project_edit && (!args.projects_remove.is_empty() || args.projects_add.len() > 1) {
        return Err(AppError::invalid_input(
            "Project field edits require one Project and cannot remove membership",
        ));
    }
    let other_changes = args.title.is_some()
        || change.is_some()
        || !args.assignees_add.is_empty()
        || !args.assignees_remove.is_empty()
        || !labels_add.is_empty()
        || !args.labels_remove.is_empty()
        || args.milestone.is_some()
        || args.clear_milestone
        || !args.projects_add.is_empty()
        || !args.projects_remove.is_empty()
        || !args.attachments.is_empty()
        || fields.iter().any(|(_, value)| value != "@auto")
        || !clears.is_empty()
        || !native.is_empty()
        || transition.is_some();
    if !other_changes && !automatic && !automatic_fields {
        return Err(AppError::invalid_input("edit requires a field to change"));
    }
    let defaults = support::github_issue_defaults(&repo)?;
    let profile = defaults
        .as_ref()
        .and_then(|defaults| defaults.project.as_ref());
    if project_edit && profile.is_none() {
        return Err(AppError::invalid_input(
            "Project field edits require an in-scope configured GitHub Project",
        ));
    }
    let model = (automatic || automatic_fields)
        .then(crate::decision_model::DecisionModel::from_environment)
        .transpose()?;
    let provider = GitHubIssues::new(repo.clone());
    provider.authenticate()?;
    let mut plan = match profile {
        Some(profile) if project_edit => Some(provider.plan_project_edit(
            profile,
            fields,
            clears,
            args.projects_add.first().cloned(),
        )?),
        _ => None,
    };
    let mut candidates = if automatic {
        filter_label_catalog(
            provider.labels()?,
            defaults
                .as_ref()
                .map(|value| value.label_candidates.as_slice())
                .unwrap_or(&[]),
        )?
        .into_iter()
        .map(|label| DecisionCandidate {
            name: label.name,
            description: label.description,
        })
        .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let label_candidate_count = candidates.len();
    if let Some(plan) = plan.as_ref() {
        let options = plan.candidates();
        if candidates
            .iter()
            .any(|label| options.iter().any(|option| option.name == label.name))
        {
            return Err(AppError::config(
                "an automatically selected GitHub Project field collides with a repository label",
            ));
        }
        candidates.extend(options);
    }
    let manual_label_count = labels_add.len();
    let mut patch = IssuePatch {
        title: args.title,
        body: change,
        assignees_add: args.assignees_add,
        assignees_remove: args.assignees_remove,
        labels_add,
        labels_remove: args.labels_remove,
        milestone: args.milestone,
        clear_milestone: args.clear_milestone,
        projects_add: args.projects_add,
        projects_remove: args.projects_remove,
        attachments: args.attachments,
        expect_updated_at: args.expect_updated_at,
    };
    let mut results = Vec::with_capacity(numbers.len());
    let mut original_guard = patch.expect_updated_at.take();
    for (index, number) in numbers.iter().copied().enumerate() {
        let result = (|| {
            patch.labels_add.truncate(manual_label_count);
            if let Some(model) = model.as_ref() {
                let current = provider.show(number)?;
                if original_guard
                    .as_deref()
                    .is_some_and(|expected| expected != current.updated_at)
                {
                    return Err(AppError::conflict());
                }
                let body = match patch.body.as_ref() {
                    Some(change) => resolve_body_change(&current.body, change)?,
                    None => current.body.clone(),
                };
                let scores = model.suggest(crate::decision_model::DecisionInput {
                    title: patch.title.as_deref().unwrap_or(&current.title),
                    description: &body,
                    candidates: &candidates,
                })?;
                if let Some(plan) = plan.as_mut() {
                    plan.choose_from_scores(&scores, AUTO_LABEL_THRESHOLD)?;
                }
                patch.labels_add.extend(automatic_label_names(
                    &scores,
                    &candidates[..label_candidate_count],
                ));
                if patch.labels_add.is_empty()
                    && !other_changes
                    && !plan.as_ref().is_some_and(|plan| plan.has_writes())
                {
                    return Ok(current);
                }
                patch.expect_updated_at = Some(current.updated_at);
            } else {
                patch.expect_updated_at = original_guard.take();
            }
            provider.edit_native(
                number,
                &patch,
                &native,
                plan.as_ref().filter(|plan| plan.has_writes()),
                transition.as_ref(),
            )
        })();
        if model.is_some() {
            patch.expect_updated_at = None;
        } else {
            original_guard = patch.expect_updated_at.take();
        }
        match result {
            Ok(issue) => results.push(issue),
            Err(error) if index == 0 => return Err(error),
            Err(error) => {
                let completed = numbers[..index]
                    .iter()
                    .map(|number| format!("https://github.com/{repo}/issues/{number}"))
                    .collect::<Vec<_>>();
                let pending = numbers[index..]
                    .iter()
                    .map(|number| format!("https://github.com/{repo}/issues/{number}"))
                    .collect::<Vec<_>>();
                return Err(AppError::partial_success(
                    serde_json::json!({ "type": "issue_batch", "issues": results,
                        "failed": pending[0], "failure": { "code": error.code, "details": error.details } }),
                    &completed,
                    &pending,
                ));
            }
        }
    }
    if numbers.len() == 1 {
        Ok(SuccessOutput::Issue(
            results.pop().expect("one issue result"),
        ))
    } else {
        Ok(SuccessOutput::IssueEdits(results))
    }
}

fn relation_arguments(references: Vec<IssueReference>, repo: &str) -> Vec<String> {
    let mut values = Vec::with_capacity(references.len());
    for reference in references {
        let value = match reference.repo {
            Some(other) if !other.eq_ignore_ascii_case(repo) => {
                format!("https://github.com/{other}/issues/{}", reference.number)
            }
            _ => reference.number.to_string(),
        };
        if !values.contains(&value) {
            values.push(value);
        }
    }
    values
}

/// Parses the repeated Project field flags of `issue edit` without touching the network.
///
/// Both lists are empty when neither flag was given, and the edit then leaves Project state
/// untouched. Repeats and set/clear overlaps are rejected here, before any remote call, because
/// the command has no sensible precedence rule for them.
fn project_field_edit(
    assignments: &[String],
    clears: &[String],
) -> Result<(Vec<(String, String)>, Vec<String>), AppError> {
    let mut fields = Vec::with_capacity(assignments.len());
    for assignment in assignments {
        let (name, value) = assignment
            .split_once('=')
            .filter(|(name, value)| !name.trim().is_empty() && !value.trim().is_empty())
            .ok_or(AppError::invalid_input(
                "--project-field must be NAME=VALUE",
            ))?;
        let name = name.trim();
        if fields.iter().any(|(field, _)| field == name) {
            return Err(AppError::invalid_input(
                "--project-field may not assign the same field twice",
            ));
        }
        fields.push((name.to_owned(), value.trim().to_owned()));
    }
    let mut names = Vec::with_capacity(clears.len());
    for name in clears {
        let name = name.trim();
        if names.iter().any(|clear| clear == name) {
            return Err(AppError::invalid_input(
                "--clear-project-field may not name the same field twice",
            ));
        }
        if fields.iter().any(|(field, _)| field == name) {
            return Err(AppError::invalid_input(
                "--project-field and --clear-project-field cannot name the same field",
            ));
        }
        names.push(name.to_owned());
    }
    Ok((fields, names))
}
