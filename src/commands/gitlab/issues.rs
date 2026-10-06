use std::collections::HashSet;

use crate::cli::GlobalArgs;
use crate::cli::gitlab::issues::{CreateArgs, IssueAction, IssueArgs, ListArgs, UpdateArgs};
use crate::commands::support;
use crate::domain::{AppError, Issue};
use crate::output::{self, SuccessOutput};
use crate::providers::IssueQuery;
use crate::providers::gitlab::issues::{
    GitLabIssueCreate, GitLabIssueUpdate, GitLabIssues,
};

pub(super) fn execute(globals: &GlobalArgs, args: IssueArgs) -> Result<(), AppError> {
    let output = match args.action {
        IssueAction::Create(args) => SuccessOutput::Issue(create(globals, args)?),
        IssueAction::List(args) => {
            let provider = provider(globals)?;
            SuccessOutput::Issues(provider.list(&query(&args))?)
        }
        IssueAction::View { number } => {
            let provider = provider(globals)?;
            SuccessOutput::Issue(provider.show(number.0)?)
        }
        IssueAction::Update(args) => SuccessOutput::Issue(update(globals, args)?),
    };
    output::write(globals.format, &output)
}

fn create(globals: &GlobalArgs, args: CreateArgs) -> Result<Issue, AppError> {
    let description = support::optional_text(
        args.description.as_deref(),
        args.description_file.as_deref(),
        "use either --description or --description-file",
    )?
    .ok_or(AppError::invalid_input(
        "issue create requires --description or --description-file",
    ))?;
    validate_description(&description)?;
    validate_create_assignees(&args.assignees)?;
    let provider = provider(globals)?;
    provider.create(&GitLabIssueCreate {
        title: args.title,
        description,
        labels: args.labels,
        assignees: args.assignees,
        milestone: args.milestone,
        confidential: args.confidential,
        weight: args.weight,
        due_date: args.due_date,
    })
}

fn update(globals: &GlobalArgs, args: UpdateArgs) -> Result<Issue, AppError> {
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
    provider.update(
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
    )
}

fn validate_description(description: &str) -> Result<(), AppError> {
    if description == "-" {
        return Err(AppError::invalid_input(
            "a description consisting only of '-' cannot be passed to glab",
        ));
    }
    Ok(())
}

fn validate_create_assignees(assignees: &[String]) -> Result<(), AppError> {
    if assignees
        .iter()
        .flat_map(|value| value.split(','))
        .any(|username| username.is_empty() || matches!(username.as_bytes()[0], b'+' | b'-' | b'!'))
    {
        return Err(AppError::invalid_input(
            "issue create assignees must be plain usernames without +/-/! prefixes",
        ));
    }
    Ok(())
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

/// Every set field maps to one `glab issue list` filter.
fn query(args: &ListArgs) -> IssueQuery {
    let state = if args.all {
        "all"
    } else if args.closed {
        "closed"
    } else {
        "open"
    };
    IssueQuery {
        state: state.to_owned(),
        limit: args.per_page,
        labels: args.labels.clone(),
        assignee: args.assignee.clone(),
        author: args.author.clone(),
        mention: None,
        milestone: args.milestone.clone(),
        search: args.search.clone(),
        issue_type: None,
    }
}

fn provider(globals: &GlobalArgs) -> Result<GitLabIssues, AppError> {
    let provider =
        GitLabIssues::new(support::resolve_repo(globals.provider, globals.repo.as_deref())?);
    provider.authenticate()?;
    Ok(provider)
}
