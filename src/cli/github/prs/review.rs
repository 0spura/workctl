use clap::{ArgGroup, Args};

use super::shared::PrNumber;

#[derive(Debug, Args)]
#[command(group = ArgGroup::new("event").required(true).args(["approve", "request_changes", "comment"]))]
pub struct ReviewArgs {
    /// Pull request number
    pub number: PrNumber,
    /// Approve the pull request
    #[arg(long)]
    pub approve: bool,
    /// Request changes
    #[arg(long = "request-changes")]
    pub request_changes: bool,
    /// Leave a review comment without approving or requesting changes
    #[arg(long)]
    pub comment: bool,
    /// Review body text
    #[arg(long, short = 'b')]
    pub body: Option<String>,
    /// Read the review body from a file; `-` reads standard input
    #[arg(long = "body-file", short = 'F', value_name = "FILE")]
    pub body_file: Option<String>,
    /// Attach a review comment to one source line at `PATH:LINE[:left|right]`, where the side
    /// defaults to `right`; may be repeated
    #[arg(long, num_args = 2, value_names = ["LOCATION", "TEXT"], action = clap::ArgAction::Append)]
    pub inline: Vec<String>,
    /// Attach a review comment whose body is read from `FILE` at `PATH:LINE[:left|right]`;
    /// `-` reads standard input; may be repeated
    #[arg(long = "inline-file", num_args = 2, value_names = ["LOCATION", "FILE"], action = clap::ArgAction::Append)]
    pub inline_file: Vec<String>,
}
