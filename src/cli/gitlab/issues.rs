use clap::{Args, Subcommand};

use crate::cli::common::{self, IssueNumber};

#[derive(Debug, Args)]
#[command(
    disable_help_subcommand = true,
    after_help = "GitLab issue operations mirror `glab issue`:\n  \
workctl issue list --all --label bug\n  \
workctl issue list --assignee @me --search crash\n  \
workctl issue view 12\n\n\
`glab` names the filters differently from `gh`: --closed/--all select the state and --per-page\n\
bounds the result, so `gh`'s --state and --limit are not accepted here."
)]
pub struct IssueArgs {
    #[command(subcommand)]
    pub action: IssueAction,
}

#[derive(Debug, Subcommand)]
pub enum IssueAction {
    /// Create an issue
    Create(CreateArgs),
    /// List issue summaries (never merge requests)
    List(ListArgs),
    /// View one issue with its description
    View {
        /// Issue number (GitLab IID)
        number: IssueNumber,
    },
    /// Update one issue
    Update(UpdateArgs),
}

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

fn parse_due_date(value: &str) -> Result<String, &'static str> {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes[..4].iter().all(u8::is_ascii_digit)
        || !bytes[5..7].iter().all(u8::is_ascii_digit)
        || !bytes[8..].iter().all(u8::is_ascii_digit)
    {
        return Err("due-date must use YYYY-MM-DD");
    }
    let year = value[..4].parse::<u16>().map_err(|_| "invalid due-date")?;
    let month = value[5..7].parse::<u8>().map_err(|_| "invalid due-date")?;
    let day = value[8..10].parse::<u8>().map_err(|_| "invalid due-date")?;
    let leap = year != 0 && year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return Err("due-date must be a valid calendar date"),
    };
    if year == 0 || !(1..=max_day).contains(&day) {
        return Err("due-date must be a valid calendar date");
    }
    Ok(value.to_owned())
}

fn parse_update_milestone(value: &str) -> Result<String, &'static str> {
    if value.is_empty() || !value.trim().is_empty() {
        Ok(value.to_owned())
    } else {
        Err("milestone must not be blank unless it is empty to clear")
    }
}

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
