use clap::Args;

use crate::cli::common::{self, IssueNumber};

use super::shared::parse_due_date;

#[derive(Debug, Args)]
pub struct UpdateArgs {
    /// Issue number (GitLab IID)
    pub number: IssueNumber,
    /// New issue title
    #[arg(long, value_parser = common::parse_non_blank)]
    pub title: Option<String>,
    /// Replace the issue description
    #[arg(long, value_parser = common::parse_non_blank, conflicts_with = "description_file")]
    pub description: Option<String>,
    /// Read the replacement description from a file; `-` reads standard input
    #[arg(long = "description-file", value_name = "FILE")]
    pub description_file: Option<String>,
    /// Add a label; may be repeated
    #[arg(long = "label", value_name = "NAME", value_parser = common::parse_non_blank)]
    pub labels: Vec<String>,
    /// Remove a label; may be repeated
    #[arg(long = "unlabel", value_name = "NAME", value_parser = common::parse_non_blank)]
    pub unlabels: Vec<String>,
    /// Add, remove (`+`/`-`/`!` prefix), or replace assignees
    #[arg(long = "assignee", value_name = "USERNAME", value_parser = common::parse_non_blank)]
    pub assignees: Vec<String>,
    /// Remove all assignees
    #[arg(long)]
    pub unassign: bool,
    /// Assign a milestone by title/global ID; empty or 0 clears it
    #[arg(long, value_parser = parse_update_milestone)]
    pub milestone: Option<String>,
    /// Make the issue confidential
    #[arg(long, conflicts_with = "public")]
    pub confidential: bool,
    /// Make the issue public
    #[arg(long, conflicts_with = "confidential")]
    pub public: bool,
    /// Set issue weight (zero or greater)
    #[arg(long, value_parser = clap::value_parser!(u64))]
    pub weight: Option<u64>,
    /// Set due date (YYYY-MM-DD)
    #[arg(long = "due-date", value_name = "DATE", value_parser = parse_due_date)]
    pub due_date: Option<String>,
}

fn parse_update_milestone(value: &str) -> Result<String, &'static str> {
    if value.is_empty() || !value.trim().is_empty() {
        Ok(value.to_owned())
    } else {
        Err("milestone must not be blank unless it is empty to clear")
    }
}
