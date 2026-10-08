use crate::cli::github::prs::CreateArgs;
use crate::cli::GlobalArgs;
use crate::domain::AppError;
use crate::output::SuccessOutput;
use crate::providers::{NewPr, PullRequestProvider};

use crate::commands::support;

use super::shared;

pub(super) fn execute(globals: &GlobalArgs, args: CreateArgs) -> Result<SuccessOutput, AppError> {
    let body = support::optional_text(
        args.body.as_deref(),
        args.body_file.as_deref(),
        "use either --body or --body-file",
    )?
    .unwrap_or_default();
    let provider = shared::provider(globals)?;
    Ok(SuccessOutput::PullRequest(provider.create(&NewPr {
        title: args.title,
        body,
        base: args.base,
        head: args.head,
        draft: args.draft,
        closes: args.closes,
        assignees: args.assignees,
        labels: args.labels,
        reviewers: args.reviewers,
        milestone: args.milestone,
        projects: args.projects,
        attachments: args.attachments,
    })?))
}
