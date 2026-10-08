use crate::cli::GlobalArgs;
use crate::cli::github::issues::ListArgs;
use crate::domain::AppError;
use crate::output::SuccessOutput;
use crate::providers::{IssueQuery, WorkItemProvider};

use super::shared;

pub(super) fn execute(globals: &GlobalArgs, args: &ListArgs) -> Result<SuccessOutput, AppError> {
    let query = query(args)?;
    let provider = shared::provider(globals)?;
    Ok(SuccessOutput::Issues(provider.list(&query)?))
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
