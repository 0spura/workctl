use crate::cli::GlobalArgs;
use crate::cli::github::issues::LockArgs;
use crate::domain::AppError;
use crate::output::SuccessOutput;
use crate::providers::WorkItemProvider;

use super::shared;

pub(super) fn execute(globals: &GlobalArgs, args: LockArgs) -> Result<SuccessOutput, AppError> {
    let provider = shared::provider(globals)?;
    if args.undo {
        provider.unlock(args.number.0)?;
    } else {
        provider.lock(args.number.0, args.reason.as_deref())?;
    }
    Ok(SuccessOutput::ConversationLock {
        number: args.number.0,
        target: "issue".to_owned(),
        locked: !args.undo,
    })
}
