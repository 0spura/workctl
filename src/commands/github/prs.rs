use crate::cli::GlobalArgs;
use crate::cli::github::prs::{ListArgs, MergeMethodArg, PrAction, PrArgs, ReviewArgs};
use crate::domain::AppError;
use crate::output::{self, SuccessOutput};
use crate::providers::github::prs::GitHubPulls;
use crate::providers::{MergeMethod, NewPr, PrPatch, PrQuery, PullRequestProvider, ReviewEvent};

use crate::commands::support;

pub(super) fn execute(globals: &GlobalArgs, args: PrArgs) -> Result<(), AppError> {
    let output = match args.action {
        PrAction::Create(args) => {
            let body = support::optional_text(
                args.body.as_deref(),
                args.body_file.as_deref(),
                "use either --body or --body-file",
            )?
            .unwrap_or_default();
            let provider = provider(globals)?;
            SuccessOutput::PullRequest(provider.create(&NewPr {
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
            })?)
        }
        PrAction::List(args) => {
            let query = query(&args)?;
            let provider = provider(globals)?;
            SuccessOutput::PullRequests(provider.list(&query)?)
        }
        PrAction::View { number } => {
            let provider = provider(globals)?;
            SuccessOutput::PullRequest(provider.show(number.0)?)
        }
        PrAction::Diff(args) => {
            let provider = provider(globals)?;
            SuccessOutput::Diff {
                number: args.number.0,
                diff: provider.diff(args.number.0, args.name_only)?,
            }
        }
        PrAction::Checks(args) => {
            let provider = provider(globals)?;
            SuccessOutput::Checks(provider.checks(args.number.0, args.required)?)
        }
        PrAction::Review(args) => {
            let event = review_event(&args);
            let body = support::optional_text(
                args.body.as_deref(),
                args.body_file.as_deref(),
                "use either --body or --body-file",
            )?;
            let provider = provider(globals)?;
            provider.review(args.number.0, event, body.as_deref())?;
            SuccessOutput::Review {
                number: args.number.0,
                event: event.as_str().to_owned(),
            }
        }
        PrAction::Merge(args) => {
            let method = args.method.map(merge_method);
            let provider = provider(globals)?;
            provider.merge(args.number.0, method, args.delete_branch, args.auto)?;
            SuccessOutput::Merge {
                number: args.number.0,
                method: args.method.map(|method| method.as_str().to_owned()),
                auto: args.auto,
            }
        }
        PrAction::Edit(args) => {
            if args.clear_milestone && args.milestone.is_some() {
                return Err(AppError::invalid_input(
                    "--milestone and --clear-milestone cannot be used together",
                ));
            }
            let change = support::body_change(&args.change)?;
            let patch = PrPatch {
                title: args.title,
                body: change,
                base: args.base,
                labels_add: args.add_label,
                labels_remove: args.remove_label,
                reviewers_add: args.add_reviewer,
                reviewers_remove: args.remove_reviewer,
                assignees_add: args.add_assignee,
                assignees_remove: args.remove_assignee,
                milestone: args.milestone,
                clear_milestone: args.clear_milestone,
                projects_add: args.projects_add,
                projects_remove: args.projects_remove,
                attachments: args.attachments,
                expect_updated_at: args.expect_updated_at,
            };
            if patch.is_empty() {
                return Err(AppError::invalid_input("update requires a field to change"));
            }
            let provider = provider(globals)?;
            SuccessOutput::PullRequest(provider.edit(args.number.0, &patch)?)
        }
        PrAction::Ready(args) => {
            let provider = provider(globals)?;
            provider.set_ready(args.number.0, args.undo)?;
            SuccessOutput::Ready {
                number: args.number.0,
                draft: args.undo,
            }
        }
        PrAction::Close(args) => {
            let provider = provider(globals)?;
            provider.close(args.number.0, args.comment.as_deref(), args.delete_branch)?;
            SuccessOutput::State {
                number: args.number.0,
                state: "closed".to_owned(),
            }
        }
        PrAction::Reopen(args) => {
            let provider = provider(globals)?;
            provider.reopen(args.number.0, args.comment.as_deref())?;
            SuccessOutput::State {
                number: args.number.0,
                state: "open".to_owned(),
            }
        }
    };
    output::write(globals.format, &output)
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

fn review_event(args: &ReviewArgs) -> ReviewEvent {
    if args.approve {
        ReviewEvent::Approve
    } else if args.request_changes {
        ReviewEvent::RequestChanges
    } else {
        ReviewEvent::Comment
    }
}

fn merge_method(method: MergeMethodArg) -> MergeMethod {
    match method {
        MergeMethodArg::Merge => MergeMethod::Merge,
        MergeMethodArg::Squash => MergeMethod::Squash,
        MergeMethodArg::Rebase => MergeMethod::Rebase,
    }
}

fn provider(globals: &GlobalArgs) -> Result<GitHubPulls, AppError> {
    let provider = GitHubPulls::new(support::resolve_repo(globals.provider, globals.repo.as_deref())?);
    provider.authenticate()?;
    Ok(provider)
}
