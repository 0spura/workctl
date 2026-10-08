use crate::cli::GlobalArgs;
use crate::cli::github::issues::CloseArgs;
use crate::domain::AppError;
use crate::output::SuccessOutput;
use crate::providers::WorkItemProvider;

use super::shared;

pub(super) fn execute(globals: &GlobalArgs, args: CloseArgs) -> Result<SuccessOutput, AppError> {
    let provider = shared::provider(globals)?;
    let duplicate_of = args
        .duplicate_of
        .map(|reference| reference.into_argument());
    provider.close(
        args.number.0,
        args.comment.as_deref(),
        args.reason.as_deref(),
        duplicate_of.as_deref(),
    )?;
    Ok(SuccessOutput::IssueState {
        number: args.number.0,
        state: "closed".to_owned(),
    })
}
