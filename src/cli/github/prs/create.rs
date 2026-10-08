use clap::Args;

use crate::cli::common;

#[derive(Debug, Args)]
pub struct CreateArgs {
    /// Pull request title; must not be blank
    #[arg(long, short = 't', value_parser = common::parse_non_blank)]
    pub title: String,
    /// Pull request body text
    #[arg(long, short = 'b')]
    pub body: Option<String>,
    /// Read the body from a file; `-` reads standard input
    #[arg(long = "body-file", short = 'F', value_name = "FILE")]
    pub body_file: Option<String>,
    /// Branch the pull request merges into; defaults to the repository default branch
    #[arg(long)]
    pub base: Option<String>,
    /// Branch holding the commits; defaults to the current branch
    #[arg(long)]
    pub head: Option<String>,
    /// Open as a draft
    #[arg(long)]
    pub draft: bool,
    /// Issue closed when the pull request merges; may be repeated
    #[arg(long = "closes", value_name = "NUMBER", value_parser = clap::value_parser!(u64).range(1..))]
    pub closes: Vec<u64>,
    /// Add an assignee; may be repeated
    #[arg(long = "assignee", value_name = "LOGIN", value_parser = common::parse_non_blank)]
    pub assignees: Vec<String>,
    /// Add a label; may be repeated
    #[arg(long = "label", value_name = "NAME", value_parser = common::parse_non_blank)]
    pub labels: Vec<String>,
    /// Request a review from a login; may be repeated
    #[arg(long = "reviewer", value_name = "LOGIN", value_parser = common::parse_non_blank)]
    pub reviewers: Vec<String>,
    /// Set the milestone by name, or `@current` for the nearest open milestone
    /// due today or later
    #[arg(long, short = 'm', value_parser = common::parse_non_blank)]
    pub milestone: Option<String>,
    /// Add to a project; may be repeated
    #[arg(long = "project", value_name = "TITLE", value_parser = common::parse_non_blank)]
    pub projects: Vec<String>,
    /// Attach an image or video; may be repeated, optionally as FILE#ALT
    #[arg(long = "attach", value_name = "FILE[#ALT]")]
    pub attachments: Vec<String>,
}
