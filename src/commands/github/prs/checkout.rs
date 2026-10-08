use crate::cli::github::prs::CheckoutArgs;
use crate::cli::GlobalArgs;
use crate::domain::AppError;
use crate::providers::PullRequestProvider;

use super::shared;

pub(super) fn execute(globals: &GlobalArgs, args: CheckoutArgs) -> Result<(), AppError> {
    let provider = shared::provider(globals)?;
    provider.checkout(args.number.0)
}
