use clap::{Args, ValueEnum};

use crate::cli::common;

#[derive(Debug, Args)]
#[command(
    after_help = "Filters pass straight to `gh pr list`. All of them are optional and combine:\n  \
workctl pr list --state merged --author me --search \"in:title workctl\""
)]
pub struct ListArgs {
    /// Pull request state to list
    #[arg(long, value_enum, default_value = "open")]
    pub state: PrListState,
    /// Maximum number of pull requests to return; defaults to defaults.github.listLimit, then 30
    #[arg(long, value_parser = common::parse_limit)]
    pub limit: Option<usize>,
    /// Filter by label; may be repeated
    #[arg(long = "label", value_name = "NAME", value_parser = common::parse_non_blank)]
    pub labels: Vec<String>,
    /// Filter by assignee login
    #[arg(long, value_parser = common::parse_non_blank)]
    pub assignee: Option<String>,
    /// Filter by author login
    #[arg(long, value_parser = common::parse_non_blank)]
    pub author: Option<String>,
    /// Filter by base branch
    #[arg(long, value_parser = common::parse_non_blank)]
    pub base: Option<String>,
    /// Filter by head branch
    #[arg(long, value_parser = common::parse_non_blank)]
    pub head: Option<String>,
    /// GitHub search query
    #[arg(long, value_parser = common::parse_non_blank)]
    pub search: Option<String>,
    /// Filter by draft state
    #[arg(long)]
    pub draft: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum PrListState {
    Open,
    Closed,
    Merged,
    All,
}

impl PrListState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Closed => "closed",
            Self::Merged => "merged",
            Self::All => "all",
        }
    }
}
