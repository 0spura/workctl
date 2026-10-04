use std::str::FromStr;

use clap::Args;

/// Body-change flags shared by `issue edit` and `pr edit`.
///
/// The item is fetched once, the change is applied to that text, and one write is sent, so a
/// caller never has to reproduce the current body.
#[derive(Debug, Args)]
pub struct BodyChangeArgs {
    /// Replace the whole body with this text
    #[arg(long)]
    pub body: Option<String>,
    /// Replace the whole body with a file's contents; `-` reads standard input
    #[arg(long = "body-file", value_name = "FILE")]
    pub body_file: Option<String>,
    /// Append this text as a new block
    #[arg(long = "append-body")]
    pub append_body: Option<String>,
    /// Append a file's contents as a new block; `-` reads standard input
    #[arg(long = "append-body-file", value_name = "FILE")]
    pub append_body_file: Option<String>,
    /// Replace the content of one ATX section, e.g. `## Acceptance`
    #[arg(long = "replace-section", value_name = "HEADING")]
    pub replace_section: Option<String>,
    /// New section content; requires --replace-section
    #[arg(long = "section-body")]
    pub section_body: Option<String>,
    /// New section content from a file; requires --replace-section
    #[arg(long = "section-body-file", value_name = "FILE")]
    pub section_body_file: Option<String>,
    /// Apply a unified diff to the current body; `-` reads standard input
    #[arg(long = "patch-file", value_name = "FILE")]
    pub patch_file: Option<String>,
}

pub fn parse_limit(value: &str) -> Result<usize, &'static str> {
    let limit = value
        .parse::<usize>()
        .map_err(|_| "limit must be an integer")?;
    if (1..=1000).contains(&limit) {
        Ok(limit)
    } else {
        Err("limit must be between 1 and 1000")
    }
}

/// GitLab returns at most 100 items per page, so a larger request cannot be honored.
pub fn parse_per_page(value: &str) -> Result<usize, &'static str> {
    let per_page = value
        .parse::<usize>()
        .map_err(|_| "per-page must be an integer")?;
    if (1..=100).contains(&per_page) {
        Ok(per_page)
    } else {
        Err("per-page must be between 1 and 100")
    }
}

/// A positive work-item number, shared by every provider grammar.
#[derive(Debug, Clone, Copy)]
pub struct IssueNumber(pub u64);

impl FromStr for IssueNumber {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let number = value
            .parse::<u64>()
            .map_err(|_| "expected a positive issue number")?;
        if number == 0 {
            return Err("expected a positive issue number");
        }
        Ok(Self(number))
    }
}

pub fn parse_non_blank(value: &str) -> Result<String, &'static str> {
    if value.trim().is_empty() {
        Err("value must not be blank")
    } else {
        Ok(value.to_owned())
    }
}
