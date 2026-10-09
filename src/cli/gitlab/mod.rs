pub mod issues;
pub mod mr;

use clap::Subcommand;

/// The GitLab grammar: verbs and flags mirror `glab`.
#[derive(Debug, Subcommand)]
pub enum GitlabCommand {
    /// Manage GitLab issues
    Issue(issues::IssueArgs),
    /// Manage GitLab merge requests
    Mr(mr::MergeRequestArgs),
}
