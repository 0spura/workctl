use crate::cli::github::prs::UpdateBranchArgs;
use crate::cli::GlobalArgs;
use crate::domain::AppError;
use crate::output::SuccessOutput;
use crate::providers::PullRequestProvider;

use super::shared;

pub(super) fn execute(
    globals: &GlobalArgs,
    args: UpdateBranchArgs,
) -> Result<SuccessOutput, AppError> {
    let provider = shared::provider(globals)?;
    provider.update_branch(args.number.0, args.rebase)?;
    Ok(SuccessOutput::UpdateBranch {
        number: args.number.0,
        rebase: args.rebase,
    })
}
