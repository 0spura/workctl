use clap::Args;

use super::shared::PrNumber;

#[derive(Debug, Args)]
pub struct UnlockArgs {
    /// Pull request number
    pub number: PrNumber,
}
