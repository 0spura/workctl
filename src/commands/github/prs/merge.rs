use crate::cli::GlobalArgs;
use crate::cli::github::prs::{MergeArgs, MergeMethodArg};
use crate::commands::support;
use crate::domain::AppError;
use crate::output::SuccessOutput;
use crate::providers::{MergeMethod, PullRequestProvider};

use super::shared;

pub(super) fn execute(globals: &GlobalArgs, args: MergeArgs) -> Result<SuccessOutput, AppError> {
    let defaults = support::github_pr_defaults()?;
    let method = args
        .method
        .map(merge_method)
        .or_else(|| defaults.merge_method.map(configured_merge_method));
    if args.delete_branch && args.auto {
        return Err(AppError::invalid_input(
            "--delete-branch cannot be combined with --auto",
        ));
    }
    // A queued merge has not happened yet, so this invocation has no branch to delete; a configured
    // default applies to merges that complete here.
    let delete_branch = (args.delete_branch || defaults.delete_branch) && !args.auto;
    let provider = shared::provider(globals)?;
    if let Err(error) = provider.merge(args.number.0, method, delete_branch, args.auto) {
        if error.code != "partial_success" {
            return Err(error);
        }
        let operation = if provider.show(args.number.0).is_ok_and(|pull_request| {
            matches!(pull_request.state, crate::domain::PullRequestState::Merged)
        }) {
            "merged"
        } else {
            "updated"
        };
        return Err(error.with_partial_event(crate::domain::PartialEvent {
            resource: "pull_request",
            number: args.number.0,
            operation,
        }));
    }
    let confirmed_merged = !args.auto
        && provider.show(args.number.0).is_ok_and(|pull_request| {
            matches!(pull_request.state, crate::domain::PullRequestState::Merged)
        });
    Ok(SuccessOutput::Merge {
        number: args.number.0,
        method: method.map(|method| method.as_str().to_owned()),
        auto: args.auto,
        confirmed_merged,
    })
}

fn merge_method(method: MergeMethodArg) -> MergeMethod {
    match method {
        MergeMethodArg::Merge => MergeMethod::Merge,
        MergeMethodArg::Squash => MergeMethod::Squash,
        MergeMethodArg::Rebase => MergeMethod::Rebase,
    }
}

fn configured_merge_method(method: crate::config::GithubMergeMethod) -> MergeMethod {
    match method {
        crate::config::GithubMergeMethod::Merge => MergeMethod::Merge,
        crate::config::GithubMergeMethod::Squash => MergeMethod::Squash,
        crate::config::GithubMergeMethod::Rebase => MergeMethod::Rebase,
    }
}
