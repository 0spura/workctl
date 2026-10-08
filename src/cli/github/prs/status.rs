use clap::Args;

use super::shared::PrNumber;

#[derive(Debug, Args)]
#[command(
    after_help = "Reports observed PR metadata and required checks; it does not decide whether repository policy permits merging.\n\n\
An empty required-check report or unknown review/mergeability value is inconclusive.\n\
Example:\n  \
workctl pr status 42"
)]
pub struct StatusArgs {
    /// Pull request number
    pub number: PrNumber,
}
