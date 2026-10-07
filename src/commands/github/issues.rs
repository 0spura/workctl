use crate::cli::GlobalArgs;
use crate::cli::github::issues::{EditArgs, IssueAction, IssueArgs, IssueReference, ListArgs};
use crate::domain::{AppError, DecisionCandidate, DecisionScore};
use crate::output::{self, SuccessOutput};
use crate::providers::github::issues::{GitHubIssues, NativeIssueEdit};
use crate::providers::{IssuePatch, IssueQuery, NewIssue, WorkItemProvider, resolve_body_change};

use crate::commands::support;

const AUTO_LABEL_THRESHOLD: f64 = 0.8;

pub(super) fn execute(globals: &GlobalArgs, args: IssueArgs) -> Result<(), AppError> {
    let output = match args.action {
        IssueAction::Create(args) => {
            let body = support::optional_text(
                args.body.as_deref(),
                args.body_file.as_deref(),
                "use either --body or --body-file",
            )?
            .unwrap_or_default();
            let mut explicit_labels = args.labels;
            let automatic = explicit_labels.iter().any(|label| label == "@auto");
            explicit_labels.retain(|label| label != "@auto");
            let repo = support::resolve_repo(globals.provider, globals.repo.as_deref())?;
            let defaults = support::github_issue_defaults(&repo)?;
            let mut assignees = args.assignees;
            if assignees.is_empty() {
                assignees = defaults
                    .as_ref()
                    .map(|value| value.assignees.clone())
                    .unwrap_or_default();
            }
            let mut labels = defaults
                .as_ref()
                .map(|value| value.labels.clone())
                .unwrap_or_default();
            for label in explicit_labels {
                if !labels.contains(&label) {
                    labels.push(label);
                }
            }
            let cli_project_fields = args.project_fields;
            let mut project_fields = defaults
                .as_ref()
                .and_then(|value| value.project.as_ref())
                .map(|value| value.fields.clone())
                .unwrap_or_default();
            for assignment in cli_project_fields {
                let (name, value) = assignment
                    .split_once('=')
                    .filter(|(name, value)| !name.trim().is_empty() && !value.trim().is_empty())
                    .ok_or(AppError::invalid_input(
                        "--project-field must be NAME=VALUE",
                    ))?;
                project_fields.insert(name.trim().to_owned(), value.trim().to_owned());
            }
            let project_profile = defaults
                .as_ref()
                .and_then(|value| value.project.clone())
                .filter(|profile| {
                    args.projects.is_empty()
                        || !project_fields.is_empty()
                        || !profile.auto_select_fields.is_empty()
                });
            if (!project_fields.is_empty()
                || project_profile
                    .as_ref()
                    .is_some_and(|profile| !profile.auto_select_fields.is_empty()))
                && args.projects.len() > 1
            {
                return Err(AppError::invalid_input(
                    "Project field assignment requires exactly one effective Project",
                ));
            }
            let auto_project_fields = project_profile.as_ref().is_some_and(|profile| {
                profile
                    .auto_select_fields
                    .iter()
                    .any(|name| !project_fields.contains_key(name))
            });
            // Model configuration is resolved before any remote call: `@auto` and at least one
            // unclaimed Project field both require a model before the issue exists.
            let decision_model = (automatic || auto_project_fields)
                .then(crate::decision_model::DecisionModel::from_environment)
                .transpose()?;
            if !project_fields.is_empty() && project_profile.is_none() {
                return Err(AppError::invalid_input(
                    "--project-field requires an in-scope configured GitHub Project",
                ));
            }
            let project_title = (args.projects.len() == 1).then(|| args.projects[0].clone());
            let provider = GitHubIssues::new(repo);
            provider.authenticate()?;
            let mut project_plan = project_profile
                .map(|profile| {
                    provider.plan_project(
                        &profile,
                        project_fields.into_iter().collect(),
                        project_title,
                    )
                })
                .transpose()?;
            // One decision call covers the repository labels and the auto-selected Project fields.
            let mut candidates = Vec::new();
            let mut label_candidates = 0;
            if automatic {
                let catalog = filter_label_catalog(
                    provider.labels()?,
                    defaults
                        .as_ref()
                        .map(|value| value.label_candidates.as_slice())
                        .unwrap_or(&[]),
                )?;
                label_candidates = catalog.len();
                candidates.extend(catalog.into_iter().map(|label| DecisionCandidate {
                    name: label.name,
                    description: label.description,
                }));
            }
            if let Some(plan) = project_plan.as_ref() {
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
            if let Some(model) = decision_model {
                let scores = model.suggest(crate::decision_model::DecisionInput {
                    title: &args.title,
                    description: &body,
                    candidates: &candidates,
                })?;
                for label in automatic_label_names(&scores, &candidates[..label_candidates]) {
                    if !labels.contains(&label) {
                        labels.push(label);
                    }
                }
                if let Some(plan) = project_plan.as_mut() {
                    plan.choose_from_scores(&scores, AUTO_LABEL_THRESHOLD)?;
                }
            }
            let new_issue = NewIssue {
                title: args.title,
                body,
                assignees,
                labels,
                milestone: args.milestone,
                projects: args.projects,
                attachments: args.attachments,
            };
            let created = match project_plan.as_ref() {
                Some(plan) => provider.create_with_project(&new_issue, plan)?,
                None => provider.create(&new_issue)?,
            };
            SuccessOutput::Issue(created)
        }
        IssueAction::List(args) => {
            let query = query(&args)?;
            let provider = provider(globals)?;
            SuccessOutput::Issues(provider.list(&query)?)
        }
        IssueAction::View { number } => {
            let provider = provider(globals)?;
            SuccessOutput::Issue(provider.show(number.0)?)
        }
        IssueAction::Edit(args) => edit(globals, args)?,
    };
    output::write(globals.format, &output)
}

fn edit(globals: &GlobalArgs, args: EditArgs) -> Result<SuccessOutput, AppError> {
    let url_repo = args
        .targets
        .iter()
        .find_map(|target| target.repo.as_deref());
    let provider_hint = globals
        .provider
        .or_else(|| url_repo.map(|_| crate::config::Provider::Github));
    let repo = support::resolve_repo(provider_hint, globals.repo.as_deref().or(url_repo))?;
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
        || project_edit
        || !native.is_empty();
    if !other_changes && !automatic {
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
    let model = automatic
        .then(crate::decision_model::DecisionModel::from_environment)
        .transpose()?;
    let provider = GitHubIssues::new(repo.clone());
    provider.authenticate()?;
    let plan = match profile {
        Some(profile) if project_edit => Some(provider.plan_project_edit(
            profile,
            fields,
            clears,
            args.projects_add.first().cloned(),
        )?),
        _ => None,
    };
    let candidates = if automatic {
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
                patch
                    .labels_add
                    .extend(automatic_label_names(&scores, &candidates));
                if patch.labels_add.is_empty() && !other_changes {
                    return Ok(current);
                }
                patch.expect_updated_at = Some(current.updated_at);
            } else {
                patch.expect_updated_at = original_guard.take();
            }
            provider.edit_native(number, &patch, &native, plan.as_ref())
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

fn filter_label_catalog(
    catalog: Vec<crate::domain::RepositoryLabel>,
    candidates: &[String],
) -> Result<Vec<crate::domain::RepositoryLabel>, AppError> {
    if candidates.is_empty() {
        return Ok(catalog);
    }
    let mut selected = Vec::with_capacity(candidates.len());
    for name in candidates {
        let label = catalog
            .iter()
            .find(|label| label.name == *name)
            .ok_or(AppError::config(
                "a configured label candidate does not exist in the repository",
            ))?;
        selected.push(label.clone());
    }
    Ok(selected)
}
fn automatic_label_names(scores: &[DecisionScore], labels: &[DecisionCandidate]) -> Vec<String> {
    scores
        .iter()
        .filter(|score| score.probability >= AUTO_LABEL_THRESHOLD)
        .filter(|score| labels.iter().any(|label| label.name == score.candidate))
        .map(|score| score.candidate.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::automatic_label_names;
    use crate::domain::{DecisionCandidate, DecisionScore};

    #[test]
    fn automatic_labels_include_only_scores_at_or_above_threshold() {
        let labels = [
            DecisionCandidate {
                name: "below".to_owned(),
                description: None,
            },
            DecisionCandidate {
                name: "boundary".to_owned(),
                description: None,
            },
            DecisionCandidate {
                name: "above".to_owned(),
                description: None,
            },
        ];
        let scores = vec![
            DecisionScore {
                candidate: "below".to_owned(),
                probability: 0.79,
            },
            DecisionScore {
                candidate: "boundary".to_owned(),
                probability: 0.8,
            },
            DecisionScore {
                candidate: "above".to_owned(),
                probability: 0.91,
            },
            // A Project field option, not a repository label: never returned as a label.
            DecisionScore {
                candidate: "Priority=High".to_owned(),
                probability: 0.99,
            },
        ];
        assert_eq!(
            automatic_label_names(&scores, &labels),
            ["boundary", "above"]
        );
    }
}

fn query(args: &ListArgs) -> Result<IssueQuery, AppError> {
    Ok(IssueQuery {
        state: args.state.as_str().to_owned(),
        limit: args.limit,
        labels: args.labels.clone(),
        assignee: args.assignee.clone(),
        author: args.author.clone(),
        mention: args.mention.clone(),
        milestone: args.milestone.clone(),
        search: args.search.clone(),
        issue_type: args.issue_type.clone(),
    })
}

fn provider(globals: &GlobalArgs) -> Result<GitHubIssues, AppError> {
    let provider = GitHubIssues::new(support::resolve_repo(
        globals.provider,
        globals.repo.as_deref(),
    )?);
    provider.authenticate()?;
    Ok(provider)
}
