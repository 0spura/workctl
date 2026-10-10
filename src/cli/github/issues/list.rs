use clap::{Args, ValueEnum};

use crate::cli::common;

#[derive(Debug, Args)]
#[command(
    after_help = "Filters pass straight to `gh issue list`. All of them are optional and combine:\n  \
workctl issue list --label bug --label p1 --assignee me --search \"in:title fix\""
)]
pub struct ListArgs {
    /// Issue state to list
    #[arg(long, value_enum, default_value = "open")]
    pub state: ListState,
    /// Maximum number of issues to return; defaults to defaults.github.listLimit, then 30
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
    /// Filter by mentioned user login
    #[arg(long, value_parser = common::parse_non_blank)]
    pub mention: Option<String>,
    /// Filter by milestone name
    #[arg(long, value_parser = common::parse_non_blank)]
    pub milestone: Option<String>,
    /// GitHub search query
    #[arg(long, value_parser = common::parse_non_blank)]
    pub search: Option<String>,
    /// Filter by issue type name
    #[arg(long = "type", value_name = "NAME", value_parser = common::parse_non_blank)]
    pub issue_type: Option<String>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ListState {
    Open,
    Closed,
    All,
}

impl ListState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Closed => "closed",
            Self::All => "all",
        }
    }
}
