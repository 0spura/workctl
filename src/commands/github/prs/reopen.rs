use crate::cli::github::prs::ReopenArgs;
use crate::cli::GlobalArgs;
use crate::domain::AppError;
use crate::output::SuccessOutput;
use crate::providers::PullRequestProvider;

use super::shared;

pub(super) fn execute(globals: &GlobalArgs, args: ReopenArgs) -> Result<SuccessOutput, AppError> {
    let provider = shared::provider(globals)?;
    provider.reopen(args.number.0, args.comment.as_deref())?;
    Ok(SuccessOutput::State {
        number: args.number.0,
        state: "open".to_owned(),
    })
}
