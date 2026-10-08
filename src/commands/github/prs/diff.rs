use crate::cli::github::prs::DiffArgs;
use crate::cli::GlobalArgs;
use crate::domain::AppError;
use crate::output::SuccessOutput;
use crate::providers::PullRequestProvider;

use super::shared;

pub(super) fn execute(globals: &GlobalArgs, args: DiffArgs) -> Result<SuccessOutput, AppError> {
    let provider = shared::provider(globals)?;
    Ok(SuccessOutput::Diff {
        number: args.number.0,
        diff: provider.diff(args.number.0, args.name_only)?,
    })
}
