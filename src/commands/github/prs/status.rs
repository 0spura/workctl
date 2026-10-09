use crate::cli::GlobalArgs;
use crate::cli::github::prs::StatusArgs;
use crate::domain::AppError;
use crate::output::SuccessOutput;
use crate::providers::PullRequestProvider;

use super::shared;

pub(super) fn execute(globals: &GlobalArgs, args: StatusArgs) -> Result<SuccessOutput, AppError> {
    let provider = shared::provider(globals)?;
    Ok(SuccessOutput::PullRequestStatus(
        provider.status(args.number.0)?,
    ))
}
