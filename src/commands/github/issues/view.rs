use crate::cli::GlobalArgs;
use crate::cli::github::issues::ViewArgs;
use crate::domain::AppError;
use crate::output::SuccessOutput;

use super::shared;

pub(super) fn execute(globals: &GlobalArgs, args: ViewArgs) -> Result<SuccessOutput, AppError> {
    let provider = shared::provider(globals)?;
    Ok(SuccessOutput::GitHubIssueView(
        provider.view(args.number.0)?,
    ))
}
