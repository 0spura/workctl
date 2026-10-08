use clap::Args;

use crate::cli::common;

#[derive(Debug, Args)]
#[command(
    after_help = "Filters pass straight to `glab issue list`. All of them are optional and combine:\n  \
workctl issue list --all --label bug --assignee @me"
)]
pub struct ListArgs {
    /// List only closed issues
    #[arg(long, conflicts_with = "all")]
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
    /// Filter by milestone title
    #[arg(long, value_parser = common::parse_non_blank)]
    pub milestone: Option<String>,
    /// Search issue title and description
    #[arg(long, value_parser = common::parse_non_blank)]
    pub search: Option<String>,
    /// Number of issues to return per page
    #[arg(long = "per-page", value_name = "N", default_value_t = 30, value_parser = common::parse_per_page)]
    pub per_page: usize,
}
