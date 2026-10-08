pub mod issues;
pub mod prs;

use clap::Subcommand;

/// The GitHub grammar: verbs and flags mirror `gh`.
#[derive(Debug, Subcommand)]
pub enum GithubCommand {
    /// Manage GitHub issues
    Issue(issues::IssueArgs),
    /// Manage GitHub pull requests
    Pr(prs::PrArgs),
}
