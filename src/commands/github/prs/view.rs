use crate::cli::GlobalArgs;
use crate::cli::github::prs::ViewArgs;
use crate::domain::AppError;
use crate::output::SuccessOutput;
use crate::providers::PullRequestProvider;

use super::shared;

pub(super) fn execute(globals: &GlobalArgs, args: ViewArgs) -> Result<SuccessOutput, AppError> {
    let provider = shared::provider(globals)?;
    Ok(SuccessOutput::PullRequest(provider.show(args.number.0)?))
}
