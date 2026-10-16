use std::str::FromStr;

use clap::Args;

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum LifecycleState {
    Open,
    Closed,
}

impl LifecycleState {
    pub fn is_closed(self) -> bool {
        matches!(self, Self::Closed)
    }
}

/// Body-change flags shared by `issue edit` and `pr edit`.
///
/// The item is fetched once, the change is applied to that text, and one write is sent, so a
/// caller never has to reproduce the current body.
#[derive(Debug, Args)]
pub struct BodyChangeArgs {
    /// Replace the whole body with this text
    #[arg(long, short = 'b', allow_hyphen_values = true)]
    pub body: Option<String>,
    /// Replace the whole body with a file's contents; `-` reads standard input
    #[arg(long = "body-file", short = 'F', value_name = "FILE")]
    pub body_file: Option<String>,
    /// Append this text as a new block
    #[arg(long = "append-body", allow_hyphen_values = true)]
    pub append_body: Option<String>,
    /// Append a file's contents as a new block; `-` reads standard input
    #[arg(long = "append-body-file", value_name = "FILE")]
    pub append_body_file: Option<String>,
    /// Replace the content of one ATX section, e.g. `## Acceptance`
    #[arg(long = "replace-section", value_name = "HEADING")]
    pub replace_section: Option<String>,
    /// New section content; requires --replace-section
    #[arg(long = "section-body", allow_hyphen_values = true)]
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

/// Arguments shared by the GitHub and GitLab `issue develop` command.
#[derive(Debug, Args)]
#[command(
    after_help = "With --name, provide a short lowercase slug without the issue number; workctl prefixes the IID. \
The branch is remote-only unless --checkout is specified."
)]
pub struct IssueDevelopArgs {
    /// Issue number
    pub number: IssueNumber,
    /// Branch to create from; defaults to the provider's issue-branch base
    #[arg(long, value_parser = parse_non_blank, conflicts_with = "list")]
    pub base: Option<String>,
    /// Short lowercase hyphenated slug; workctl prefixes the issue IID
    #[arg(long, value_name = "SLUG", value_parser = parse_branch_slug, conflicts_with = "list")]
    pub name: Option<String>,
    /// Check out the created branch in the current worktree
    #[arg(long, conflicts_with = "list")]
    pub checkout: bool,
    /// List the branches linked to the issue
    #[arg(long)]
    pub list: bool,
}

pub fn parse_branch_slug(value: &str) -> Result<String, &'static str> {
    let mut previous_hyphen = false;
    for (index, character) in value.chars().enumerate() {
        let valid_alphanumeric = character.is_ascii_lowercase() || character.is_ascii_digit();
        if valid_alphanumeric {
            previous_hyphen = false;
            continue;
        }
        if character == '-' && index > 0 && !previous_hyphen {
            previous_hyphen = true;
            continue;
        }
        return Err("branch name must be a lowercase hyphenated slug");
    }
    if value.is_empty() || previous_hyphen {
        return Err("branch name must be a lowercase hyphenated slug");
    }
    Ok(value.to_owned())
}
