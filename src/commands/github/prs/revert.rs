use crate::cli::GlobalArgs;
use crate::cli::github::prs::RevertArgs;
use crate::domain::AppError;
use crate::output::SuccessOutput;
use crate::providers::PullRequestProvider;

use super::shared;

pub(super) fn execute(globals: &GlobalArgs, args: RevertArgs) -> Result<SuccessOutput, AppError> {
    let body = crate::commands::support::optional_text(
        args.body.as_deref(),
        args.body_file.as_deref(),
        "choose either --body or --body-file",
    )?;
    let provider = shared::provider(globals)?;
    let revert_pull_request = provider.revert(
        args.number.0,
        args.title.as_deref(),
        body.as_deref(),
        args.draft,
    )?;
    Ok(SuccessOutput::Revert {
        number: args.number.0,
        pull_request: revert_pull_request,
    })
}
