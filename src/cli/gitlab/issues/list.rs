use clap::Args;

use crate::cli::common;

#[derive(Debug, Args)]
#[command(
    after_help = "Filters pass straight to `glab issue list`. All of them are optional and combine:\n  \
workctl issue list --all --label bug --assignee @me"
)]
pub struct ListArgs {
    /// List only closed issues
    #[arg(long)]
    pub closed: bool,
    /// List issues in every state
    #[arg(long)]
    pub all: bool,
    /// Filter by label; may be repeated or comma-separated
    #[arg(long = "label", value_name = "NAME", value_parser = common::parse_non_blank)]
    pub labels: Vec<String>,
    /// Filter by assignee username, or `@me`
    #[arg(long, value_parser = common::parse_non_blank)]
    pub assignee: Option<String>,
    /// Filter by author username, or `@me`
    #[arg(long, value_parser = common::parse_non_blank)]
    pub author: Option<String>,
    /// Filter by milestone ID
    #[arg(long, value_parser = common::parse_non_blank)]
    pub milestone: Option<String>,
    /// Search issue title and description
    #[arg(long, value_parser = common::parse_non_blank)]
    pub search: Option<String>,
    /// Search only selected fields
    #[arg(long = "in", value_parser = common::parse_non_blank)]
    pub in_fields: Option<String>,
    /// Filter by confidential status
    #[arg(long)]
    pub confidential: bool,
    /// Filter by issue type
    #[arg(long, value_parser = ["issue", "incident", "test_case"])]
    pub issue_type: Option<String>,
    /// Filter by iteration ID
    #[arg(long, value_parser = common::parse_non_blank)]
    pub iteration: Option<String>,
    /// Exclude issues assigned to these users
    #[arg(long = "not-assignee", value_parser = common::parse_non_blank)]
    pub not_assignees: Vec<String>,
    /// Exclude issues authored by these users
    #[arg(long = "not-author", value_parser = common::parse_non_blank)]
    pub not_authors: Vec<String>,
    /// Exclude issues with these labels
    #[arg(long = "not-label", value_parser = common::parse_non_blank)]
    pub not_labels: Vec<String>,
    /// Order by a native GitLab issue field
    #[arg(long, value_parser = ["created_at", "updated_at", "priority", "due_date", "relative_position", "label_priority", "milestone_due", "popularity", "weight"])]
    pub order: Option<String>,
    /// Sort direction for --order
    #[arg(long, value_parser = ["asc", "desc"])]
    pub sort: Option<String>,
    /// Page number
    #[arg(long, default_value_t = 1, value_parser = parse_page)]
    pub page: usize,
    /// Number of issues to return per page
    #[arg(long = "per-page", value_name = "N", default_value_t = 30, value_parser = common::parse_per_page)]
    pub per_page: usize,
}

fn parse_page(value: &str) -> Result<usize, &'static str> {
    let page = value
        .parse::<usize>()
        .map_err(|_| "page must be a positive integer")?;
    if page == 0 {
        Err("page must be a positive integer")
    } else {
        Ok(page)
    }
}
