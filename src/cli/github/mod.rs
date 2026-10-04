pub mod issues;
pub mod prs;

use clap::Subcommand;

/// The GitHub grammar: verbs and flags mirror `gh`.
#[derive(Debug, Subcommand)]
pub enum GithubCommand {
    Issue(issues::IssueArgs),
    Pr(prs::PrArgs),
}
