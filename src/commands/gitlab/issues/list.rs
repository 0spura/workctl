use crate::cli::GlobalArgs;
use crate::cli::gitlab::issues::ListArgs;
use crate::domain::AppError;
use crate::output::SuccessOutput;
use crate::providers::IssueQuery;

use super::shared;

pub(super) fn execute(globals: &GlobalArgs, args: &ListArgs) -> Result<SuccessOutput, AppError> {
    let provider = shared::provider(globals)?;
    Ok(SuccessOutput::Issues(provider.list(&query(args))?))
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
