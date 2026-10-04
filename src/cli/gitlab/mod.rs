pub mod issues;

use clap::Subcommand;

/// The GitLab grammar: verbs and flags mirror `glab`.
#[derive(Debug, Subcommand)]
pub enum GitlabCommand {
    Issue(issues::IssueArgs),
}
