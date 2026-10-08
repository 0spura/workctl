use crate::cli::GlobalArgs;
use crate::cli::gitlab::issues::CreateArgs;
use crate::commands::support;
use crate::domain::AppError;
use crate::output::SuccessOutput;
use crate::providers::gitlab::issues::GitLabIssueCreate;

use super::shared::{provider, validate_description};

pub(super) fn execute(globals: &GlobalArgs, args: CreateArgs) -> Result<SuccessOutput, AppError> {
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
    Ok(SuccessOutput::Issue(provider.create(
        &GitLabIssueCreate {
            title: args.title,
            description,
            labels: args.labels,
            assignees: args.assignees,
            milestone: args.milestone,
            confidential: args.confidential,
            weight: args.weight,
            due_date: args.due_date,
        },
    )?))
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
