use crate::cli::github::prs::ListArgs;
use crate::cli::GlobalArgs;
use crate::domain::AppError;
use crate::output::SuccessOutput;
use crate::providers::{PrQuery, PullRequestProvider};

use super::shared;

pub(super) fn execute(globals: &GlobalArgs, args: &ListArgs) -> Result<SuccessOutput, AppError> {
    let query = query(args)?;
    let provider = shared::provider(globals)?;
    Ok(SuccessOutput::PullRequests(provider.list(&query)?))
}

fn query(args: &ListArgs) -> Result<PrQuery, AppError> {
    Ok(PrQuery {
        state: args.state.as_str().to_owned(),
        limit: args.limit,
        labels: args.labels.clone(),
        assignee: args.assignee.clone(),
        author: args.author.clone(),
        base: args.base.clone(),
        head: args.head.clone(),
        search: args.search.clone(),
        draft: args.draft,
    })
}
