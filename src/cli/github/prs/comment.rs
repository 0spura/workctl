use clap::{ArgGroup, Args};

use super::shared::PrNumber;

#[derive(Debug, Args)]
#[command(group = ArgGroup::new("text").required(true).args(["body", "body_file"]))]
pub struct CommentArgs {
    /// Pull request number
    pub number: PrNumber,
    /// Comment text
    #[arg(long, short = 'b')]
    pub body: Option<String>,
    /// Read comment text from a file; `-` reads standard input
    #[arg(long = "body-file", short = 'F', value_name = "FILE")]
    pub body_file: Option<String>,
}
