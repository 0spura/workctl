use std::collections::HashSet;

use crate::cli::GlobalArgs;
use crate::cli::gitlab::issues::UpdateArgs;
use crate::commands::labels::automatic_label_names;
use crate::commands::support;
use crate::decision_model::{DecisionInput, DecisionModel};
use crate::domain::{AppError, DecisionCandidate};
use crate::output::SuccessOutput;
use crate::providers::gitlab::issues::{GitLabIssueUpdate, GitLabIssues};

pub(super) fn execute(globals: &GlobalArgs, args: UpdateArgs) -> Result<SuccessOutput, AppError> {
    let description = support::optional_text(
        args.description.as_deref(),
        args.description_file.as_deref(),
        "use either --description or --description-file",
    )?;
    if description.as_deref() == Some("") {
        return Err(AppError::invalid_input(
            "glab issue update cannot clear an issue description with an empty value",
        ));
    }
    validate_update_assignees(&args.assignees, args.unassign)?;

    if args.unlabels.iter().any(|label| label == "@auto") {
        return Err(AppError::invalid_input(
            "--unlabel does not accept the automatic-label marker",
        ));
    }
    let mut labels = args.labels;
    let automatic = labels.iter().any(|label| label == "@auto");
    labels.retain(|label| label != "@auto");
    if args.title.is_none()
        && description.is_none()
        && labels.is_empty()
        && !automatic
        && args.unlabels.is_empty()
        && args.assignees.is_empty()
        && !args.unassign
        && args.milestone.is_none()
        && !args.confidential
        && !args.public
        && args.weight.is_none()
        && args.due_date.is_none()
    {
        return Err(AppError::invalid_input("update requires a field to change"));
    }

    let model = automatic
        .then(DecisionModel::from_environment)
        .transpose()?;
    let repo = support::resolve_repo(globals.work_item_provider, globals.repo.as_deref())?;
    let provider = GitLabIssues::new(repo)?;
    provider.authenticate()?;
    let mut unchanged_issue = None;
    let automatic_labels = if let Some(model) = model {
        let initial = provider.show(args.number.0)?;
        let candidates = provider
            .labels()?
            .into_iter()
            .map(|label| DecisionCandidate {
                name: label.name,
                description: label.description,
            })
            .collect::<Vec<_>>();
        let title = args.title.as_deref().unwrap_or(&initial.title);
        let description_text = description.as_deref().unwrap_or(&initial.body);
        let scores = model.suggest(DecisionInput {
            title,
            description: description_text,
            candidates: &candidates,
        })?;
        let selected = automatic_label_names(&scores, &candidates);
        let current = provider.show(args.number.0)?;
        if current.updated_at != initial.updated_at {
            return Err(AppError::conflict());
        }
        unchanged_issue = Some(current);
        selected
    } else {
        Vec::new()
    };
    for label in automatic_labels {
        if !labels.contains(&label) {
            labels.push(label);
        }
    }
    let patch = GitLabIssueUpdate {
        title: args.title,
        description,
        labels_add: labels,
        labels_remove: args.unlabels,
        assignees: args.assignees,
        unassign: args.unassign,
        milestone: args.milestone,
        confidential: if args.confidential {
            Some(true)
        } else if args.public {
            Some(false)
        } else {
            None
        },
        weight: args.weight,
        due_date: args.due_date,
    };
    if automatic
        && patch.labels_add.is_empty()
        && patch.title.is_none()
        && patch.description.is_none()
        && patch.labels_remove.is_empty()
        && patch.assignees.is_empty()
        && !patch.unassign
        && patch.milestone.is_none()
        && patch.confidential.is_none()
        && patch.weight.is_none()
        && patch.due_date.is_none()
    {
        let issue = match unchanged_issue {
            Some(issue) => issue,
            None => provider.show(args.number.0)?,
        };
        return Ok(SuccessOutput::Issue(issue));
    }
    Ok(SuccessOutput::Issue(
        provider.update(args.number.0, &patch)?,
    ))
}

fn validate_update_assignees(assignees: &[String], unassign: bool) -> Result<(), AppError> {
    if unassign && !assignees.is_empty() {
        return Err(AppError::invalid_input(
            "--assignee and --unassign cannot be used together",
        ));
    }
    let mut replaces = false;
    let mut relative = false;
    let mut adds = HashSet::new();
    let mut removes = HashSet::new();
    for username in assignees.iter().flat_map(|value| value.split(',')) {
        let (kind, value) = match username.as_bytes().first() {
            Some(b'+') => (1, &username[1..]),
            Some(b'-' | b'!') => (2, &username[1..]),
            _ => (0, username),
        };
        if value.is_empty() {
            return Err(AppError::invalid_input(
                "--assignee prefix must be followed by a username",
            ));
        }
        match kind {
            0 => replaces = true,
            1 => {
                relative = true;
                adds.insert(value);
            }
            _ => {
                relative = true;
                removes.insert(value);
            }
        }
    }
    if replaces && relative {
        return Err(AppError::invalid_input(
            "do not mix replacement assignees with +/-/! relative assignments",
        ));
    }
    if adds.iter().any(|assignee| removes.contains(assignee)) {
        return Err(AppError::invalid_input(
            "the same assignee cannot be both added and removed",
        ));
    }
    Ok(())
}
