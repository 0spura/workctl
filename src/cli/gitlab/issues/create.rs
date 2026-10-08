use clap::Args;

use crate::cli::common;

use super::shared::parse_due_date;

#[derive(Debug, Args)]
#[command(group(clap::ArgGroup::new("issue_description").required(true).multiple(false).args(["description", "description_file"])))]
pub struct CreateArgs {
    /// Issue title; must not be blank
    #[arg(long, value_parser = common::parse_non_blank)]
    pub title: String,
    /// Issue description
    #[arg(long)]
    pub description: Option<String>,
    /// Read the description from a file; `-` reads standard input
    #[arg(
        long = "description-file",
        value_name = "FILE",
        required_unless_present = "description"
    )]
    pub description_file: Option<String>,
    /// Add a label; may be repeated
    #[arg(long = "label", value_name = "NAME", value_parser = common::parse_non_blank)]
    pub labels: Vec<String>,
    /// Assign issue to usernames; may be repeated or comma-separated
    #[arg(long = "assignee", value_name = "USERNAME", value_parser = common::parse_non_blank)]
    pub assignees: Vec<String>,
    /// Assign a milestone by title or global ID
    #[arg(long, value_parser = common::parse_non_blank)]
    pub milestone: Option<String>,
    /// Make the issue confidential
    #[arg(long)]
    pub confidential: bool,
    /// Set issue weight (zero or greater)
    #[arg(long, value_parser = clap::value_parser!(u64))]
    pub weight: Option<u64>,
    /// Set due date (YYYY-MM-DD)
    #[arg(long = "due-date", value_name = "DATE", value_parser = parse_due_date)]
    pub due_date: Option<String>,
}
