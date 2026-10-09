use crate::cli::GlobalArgs;
use crate::cli::gitlab::issues::StateArgs;
use crate::domain::AppError;
use crate::output::SuccessOutput;

use super::shared;

pub(super) enum Operation {
    Close,
    Reopen,
    Subscribe,
    Unsubscribe,
}

pub(super) fn execute(
    globals: &GlobalArgs,
    args: StateArgs,
    operation: Operation,
) -> Result<SuccessOutput, AppError> {
    let provider = shared::provider(globals)?;
    let number = args.number.0;
    match operation {
        Operation::Close => {
            provider.set_state(number, true)?;
            Ok(SuccessOutput::IssueState {
                number,
                state: "closed".to_owned(),
            })
        }
        Operation::Reopen => {
            provider.set_state(number, false)?;
            Ok(SuccessOutput::IssueState {
                number,
                state: "open".to_owned(),
            })
        }
        Operation::Subscribe => {
            provider.set_subscription(number, true)?;
            Ok(SuccessOutput::State {
                number,
                state: "subscribed".to_owned(),
            })
        }
        Operation::Unsubscribe => {
            provider.set_subscription(number, false)?;
            Ok(SuccessOutput::State {
                number,
                state: "unsubscribed".to_owned(),
            })
        }
    }
}
