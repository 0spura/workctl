use crate::cli::GlobalArgs;
use crate::cli::gitlab::issues::ViewArgs;
use crate::domain::AppError;
use crate::output::SuccessOutput;

use super::shared;

pub(super) fn execute(globals: &GlobalArgs, args: ViewArgs) -> Result<SuccessOutput, AppError> {
    let provider = shared::provider(globals)?;
    Ok(SuccessOutput::Issue(provider.show(args.number.0)?))
}
