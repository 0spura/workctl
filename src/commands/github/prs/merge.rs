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
    provider.merge(args.number.0, method, delete_branch, args.auto)?;
    Ok(SuccessOutput::Merge {
        number: args.number.0,
        method: method.map(|method| method.as_str().to_owned()),
        auto: args.auto,
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
