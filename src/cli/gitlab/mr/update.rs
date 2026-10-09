use clap::Args;

use crate::cli::common;

#[derive(Debug, Args)]
pub struct UpdateArgs {
    /// Merge request IID
    pub number: common::IssueNumber,
    /// Replace the merge request title
    #[arg(long, value_parser = common::parse_non_blank)]
    pub title: Option<String>,
    /// Replace the merge request description
    #[arg(long, conflicts_with = "description_file")]
    pub description: Option<String>,
    /// Read replacement description from a file; `-` reads standard input
    #[arg(long = "description-file", value_name = "FILE")]
    pub description_file: Option<String>,
    /// Add an assignee; may be repeated
    #[arg(long = "assignee", short = 'a', value_parser = common::parse_non_blank)]
    pub assignees: Vec<String>,
    /// Remove all assignees
    #[arg(long)]
    pub unassign: bool,
    /// Add a label; may be repeated
    #[arg(long = "label", short = 'l', value_parser = common::parse_non_blank)]
    pub labels: Vec<String>,
    /// Remove a label; may be repeated
    #[arg(long = "unlabel", value_parser = common::parse_non_blank)]
    pub unlabels: Vec<String>,
    /// Assign milestone by title or global ID
    #[arg(long, value_parser = common::parse_non_blank)]
    pub milestone: Option<String>,
    /// Set target branch
    #[arg(long = "target-branch", short = 'b', value_parser = common::parse_non_blank)]
    pub target_branch: Option<String>,
    #[arg(long = "reviewer", value_parser = common::parse_non_blank)]
    pub reviewers: Vec<String>,
    /// Mark draft
    #[arg(long)]
    pub draft: bool,
    /// Mark ready for review
    #[arg(long)]
    pub ready: bool,
    /// Mark work in progress
    #[arg(long)]
    pub wip: bool,
    /// Unlock discussion
    #[arg(long)]
    pub unlock_discussion: bool,
    /// Lock discussion
    #[arg(long)]
    pub lock_discussion: bool,
    /// Remove source branch after merge
    #[arg(long = "remove-source-branch", action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true")]
    pub remove_source_branch: Option<bool>,
    /// Squash commits before merge
    #[arg(long = "squash-before-merge", action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true")]
    pub squash_before_merge: Option<bool>,
    /// Populate title and description from commits
    #[arg(long, requires = "fill_commit_body")]
    pub fill: bool,
    /// Include commit bodies; requires --fill
    #[arg(long = "fill-commit-body", requires = "fill")]
    pub fill_commit_body: bool,
    /// Skip any confirmation prompts
    #[arg(long, short)]
    pub yes: bool,
}
