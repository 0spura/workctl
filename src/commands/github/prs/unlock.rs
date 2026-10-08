use crate::cli::GlobalArgs;
use crate::cli::github::prs::UnlockArgs;
use crate::domain::AppError;
use crate::output::SuccessOutput;
use crate::providers::PullRequestProvider;

use super::shared;

pub(super) fn execute(globals: &GlobalArgs, args: UnlockArgs) -> Result<SuccessOutput, AppError> {
    let provider = shared::provider(globals)?;
    provider.unlock(args.number.0)?;
    Ok(SuccessOutput::ConversationLock {
        number: args.number.0,
        target: "pr".to_owned(),
        locked: false,
    })
}
