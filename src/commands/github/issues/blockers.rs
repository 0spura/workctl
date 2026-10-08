use crate::cli::GlobalArgs;
use crate::cli::github::issues::BlockersArgs;
use crate::domain::AppError;
use crate::output::SuccessOutput;

use super::shared;

pub(super) fn execute(globals: &GlobalArgs, args: BlockersArgs) -> Result<SuccessOutput, AppError> {
    let provider = shared::provider(globals)?;
    Ok(SuccessOutput::BlockerChains(
        provider.blocker_chains(args.number.0)?,
    ))
}
