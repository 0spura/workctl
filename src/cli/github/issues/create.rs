use clap::Args;

use super::references::IssueReference;
use crate::cli::common;

#[derive(Debug, Args)]
pub struct CreateArgs {
    /// Issue title; must not be blank
    #[arg(short = 't', long, value_parser = common::parse_non_blank)]
    pub title: String,
    /// Issue body text
    #[arg(short = 'b', long, allow_hyphen_values = true)]
    pub body: Option<String>,
    /// Add the new issue as a sub-issue of this parent, by number or URL
    #[arg(long, value_name = "NUMBER|URL")]
    pub parent: Option<IssueReference>,
    /// Mark the new issue as blocked by these issues; repeat or comma-separate
    #[arg(long, value_name = "NUMBER|URL", value_delimiter = ',')]
    pub blocked_by: Vec<IssueReference>,
    /// Mark the new issue as blocking these issues; repeat or comma-separate
    #[arg(long, value_name = "NUMBER|URL", value_delimiter = ',')]
    pub blocking: Vec<IssueReference>,
    /// Read the body from a file; `-` reads standard input
    #[arg(short = 'F', long = "body-file", value_name = "FILE")]
    pub body_file: Option<String>,
    /// Add an assignee; may be repeated
    #[arg(long = "assignee", value_name = "LOGIN", value_parser = common::parse_non_blank)]
    pub assignees: Vec<String>,
    /// Add an existing label; use `@auto` to add labels selected by the configured decision model
    #[arg(long = "label", value_name = "NAME", value_parser = common::parse_non_blank)]
    pub labels: Vec<String>,
    /// Set the milestone by name, or `@current` for the nearest open milestone
    /// due today or later
    #[arg(short = 'm', long, value_parser = common::parse_non_blank)]
    pub milestone: Option<String>,
    /// Add to a project; may be repeated
    #[arg(long = "project", value_name = "TITLE", value_parser = common::parse_non_blank)]
    pub projects: Vec<String>,
    /// Set a dynamic field on the configured GitHub Project; repeat as NAME=VALUE
    #[arg(long = "project-field", value_name = "NAME=VALUE", value_parser = common::parse_non_blank)]
    pub project_fields: Vec<String>,
    /// Attach an image or video; may be repeated, optionally as FILE#ALT
    #[arg(long = "attach", value_name = "FILE[#ALT]")]
    pub attachments: Vec<String>,
}
