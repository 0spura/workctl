use clap::{ArgGroup, Args};

use crate::cli::common::IssueNumber;

#[derive(Debug, Args)]
#[command(group = ArgGroup::new("text").required(true).args(["body", "body_file"]))]
pub struct CommentArgs {
    /// Issue number
    pub number: IssueNumber,
    /// Comment text
    #[arg(long, short = 'b')]
    pub body: Option<String>,
    /// Read comment text from a file; `-` reads standard input
    #[arg(long = "body-file", short = 'F', value_name = "FILE")]
    pub body_file: Option<String>,
}
