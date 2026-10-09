use crate::cli::GlobalArgs;
use crate::cli::github::prs::ReadyArgs;
use crate::domain::AppError;
use crate::output::SuccessOutput;
use crate::providers::PullRequestProvider;

use super::shared;

pub(super) fn execute(globals: &GlobalArgs, args: ReadyArgs) -> Result<SuccessOutput, AppError> {
    let provider = shared::provider(globals)?;
    provider.set_ready(args.number.0, args.undo)?;
    Ok(SuccessOutput::Ready {
        number: args.number.0,
        draft: args.undo,
    })
}
