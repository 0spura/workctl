use crate::cli::GlobalArgs;
use crate::cli::github::prs::CloseArgs;
use crate::domain::AppError;
use crate::output::SuccessOutput;
use crate::providers::PullRequestProvider;

use super::shared;

pub(super) fn execute(globals: &GlobalArgs, args: CloseArgs) -> Result<SuccessOutput, AppError> {
    let provider = shared::provider(globals)?;
    provider.close(args.number.0, args.comment.as_deref(), args.delete_branch)?;
    Ok(SuccessOutput::State {
        number: args.number.0,
        state: "closed".to_owned(),
    })
}
