use clap::Args;

use super::shared::PrNumber;

#[derive(Debug, Args)]
pub struct CloseArgs {
    /// Pull request number
    pub number: PrNumber,
    /// Leave a closing comment
    #[arg(long)]
    pub comment: Option<String>,
    /// Delete the local and remote branch after closing
    #[arg(long = "delete-branch")]
    pub delete_branch: bool,
}
