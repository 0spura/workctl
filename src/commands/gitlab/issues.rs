use crate::cli::GlobalArgs;
use crate::cli::gitlab::issues::{IssueAction, IssueArgs, ListArgs};
use crate::commands::support;
use crate::domain::AppError;
use crate::output::{self, SuccessOutput};
use crate::providers::IssueQuery;
use crate::providers::gitlab::issues::GitLabIssues;

pub(super) fn execute(globals: &GlobalArgs, args: IssueArgs) -> Result<(), AppError> {
    let output = match args.action {
        IssueAction::List(args) => {
            let provider = provider(globals)?;
            SuccessOutput::Issues(provider.list(&query(&args))?)
        }
        IssueAction::View { number } => {
            let provider = provider(globals)?;
            SuccessOutput::Issue(provider.show(number.0)?)
        }
    };
    output::write(globals.format, &output)
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
