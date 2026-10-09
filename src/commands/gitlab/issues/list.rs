use crate::cli::GlobalArgs;
use crate::cli::gitlab::issues::ListArgs;
use crate::domain::AppError;
use crate::output::SuccessOutput;
use crate::providers::gitlab::issues::GitLabIssueQuery;

use super::shared;

pub(super) fn execute(globals: &GlobalArgs, args: &ListArgs) -> Result<SuccessOutput, AppError> {
    let provider = shared::provider(globals)?;
    Ok(SuccessOutput::Issues(provider.list(&query(args))?))
}

/// Every set field maps to one documented `glab issue list` filter.
fn query(args: &ListArgs) -> GitLabIssueQuery {
    GitLabIssueQuery {
        all: args.all,
        closed: args.closed,
        labels: args.labels.clone(),
        assignee: args.assignee.clone(),
        author: args.author.clone(),
        milestone: args.milestone.clone(),
        search: args.search.clone(),
        in_fields: args.in_fields.clone(),
        confidential: args.confidential,
        issue_type: args.issue_type.clone(),
        iteration: args.iteration.clone(),
        not_assignees: args.not_assignees.clone(),
        not_authors: args.not_authors.clone(),
        not_labels: args.not_labels.clone(),
        order: args.order.clone(),
        sort: args.sort.clone(),
        page: args.page,
        per_page: args.per_page,
    }
}
