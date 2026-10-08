use clap::Args;

use super::shared::PrNumber;

#[derive(Debug, Args)]
pub struct RevertArgs {
    /// Pull request number to revert
    pub number: PrNumber,
    /// Title for the new revert pull request
    #[arg(long)]
    pub title: Option<String>,
    /// Create the revert pull request as a draft
    #[arg(long)]
    pub draft: bool,
    /// Body for the new revert pull request
    #[arg(long, short = 'b')]
    pub body: Option<String>,
    /// Read the body from a file; `-` reads standard input
    #[arg(long = "body-file", short = 'F', value_name = "FILE")]
    pub body_file: Option<String>,
}
