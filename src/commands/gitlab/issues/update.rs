use std::collections::HashSet;

use crate::cli::GlobalArgs;
use crate::cli::gitlab::issues::UpdateArgs;
use crate::commands::support;
use crate::domain::AppError;
use crate::output::SuccessOutput;
use crate::providers::gitlab::issues::GitLabIssueUpdate;

use super::shared::{provider, validate_description};

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
    validate_description(description.as_deref().unwrap_or_default())?;
    validate_update_assignees(&args.assignees, args.unassign)?;
    if args.title.is_none()
        && description.is_none()
        && args.labels.is_empty()
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
    let provider = provider(globals)?;
    Ok(SuccessOutput::Issue(provider.update(
        args.number.0,
        &GitLabIssueUpdate {
            title: args.title,
            description,
            labels_add: args.labels,
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
        },
    )?))
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
