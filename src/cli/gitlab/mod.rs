pub mod issues;

use clap::Subcommand;

/// The GitLab grammar: verbs and flags mirror `glab`.
#[derive(Debug, Subcommand)]
pub enum GitlabCommand {
    /// Manage GitLab issues
    Issue(issues::IssueArgs),
}
