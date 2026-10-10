use crate::cli::GlobalArgs;
use crate::cli::github::issues::DevelopArgs;
use crate::domain::AppError;
use crate::output::SuccessOutput;

use super::shared;

pub(super) fn execute(globals: &GlobalArgs, args: DevelopArgs) -> Result<SuccessOutput, AppError> {
    let provider = shared::provider(globals)?;
    if args.list {
        return Ok(SuccessOutput::LinkedBranches {
            number: args.number.0,
            branches: provider.linked_branches(args.number.0)?,
        });
    }
    Ok(SuccessOutput::LinkedBranch {
        number: args.number.0,
        branch: provider.develop_branch(
            args.number.0,
            args.base.as_deref(),
            args.name.as_deref(),
            args.checkout,
        )?,
    })
}
