use std::time::Duration;

use crate::cli::github::prs::ChecksArgs;
use crate::cli::GlobalArgs;
use crate::domain::AppError;
use crate::output::SuccessOutput;
use crate::providers::{PrChecksOptions, PullRequestProvider};

use super::shared;

pub(super) fn execute(globals: &GlobalArgs, args: ChecksArgs) -> Result<SuccessOutput, AppError> {
    let provider = shared::provider(globals)?;
    let options = PrChecksOptions {
        required: args.required,
        watch: args.watch,
        interval: args.interval,
        fail_fast: args.fail_fast,
        watch_timeout: args.watch_timeout.map(Duration::from_secs),
    };
    Ok(SuccessOutput::Checks(
        provider.checks(args.number.0, &options)?,
    ))
}
