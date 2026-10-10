use crate::cli::GlobalArgs;
use crate::cli::github::prs::CreateArgs;
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
    // RF-CFG.5: configured `defaults.github.pr` values merge with the explicit flags before the
    // provider is reached.
    let defaults = support::github_pr_defaults()?;
    let base = args.base.or(defaults.base);
    let draft = args.draft || defaults.draft;
    let assignees = if args.assignees.is_empty() {
        defaults.assignees
    } else {
        args.assignees
    };
    let reviewers = if args.reviewers.is_empty() {
        defaults.reviewers
    } else {
        args.reviewers
    };
    // Configured labels precede explicit ones, with duplicates removed in first-seen order.
    let mut labels = Vec::new();
    for label in defaults.labels.iter().chain(args.labels.iter()) {
        if !labels.contains(label) {
            labels.push(label.clone());
        }
    }
    let provider = shared::provider(globals)?;
    Ok(SuccessOutput::PullRequest(provider.create(&NewPr {
        title: args.title,
        body,
        base,
        head: args.head,
        draft,
        closes: args.closes,
        assignees,
        labels,
        reviewers,
        milestone: args.milestone,
        projects: args.projects,
        attachments: args.attachments,
    })?))
}
