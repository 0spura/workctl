use clap::Args;

use super::shared::PrNumber;

#[derive(Debug, Args)]
pub struct DiffArgs {
    /// Pull request number
    pub number: PrNumber,
    /// Print only the names of changed files
    #[arg(long = "name-only")]
    pub name_only: bool,
}
