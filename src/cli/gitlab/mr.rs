mod update;
pub use update::UpdateArgs;

use clap::{Args, Subcommand};

use crate::cli::common;

#[derive(Debug, Args)]
#[command(disable_help_subcommand = true)]
pub struct MergeRequestArgs {
    #[command(subcommand)]
    pub action: MergeRequestAction,
}

#[derive(Debug, Subcommand)]
pub enum MergeRequestAction {
    /// List merge requests
    List(ListArgs),
    /// Update one merge request
    Update(UpdateArgs),
    /// Create a merge request
    Create(CreateArgs),
    /// View one merge request
    View(ViewArgs),
    /// Print a merge request diff
    Diff(DiffArgs),
    /// Close a merge request
    Close(StateArgs),
    /// Check out a merge request branch
    Checkout(CheckoutArgs),
    /// Reopen a merge request
    Reopen(StateArgs),
    /// Approve a merge request
    Approve(StateArgs),
    /// Revoke approval on a merge request
    Revoke(StateArgs),
    /// Rebase a merge request source branch
    Rebase(StateArgs),
    /// Subscribe to merge request updates
    Subscribe(StateArgs),
    /// Unsubscribe from merge request updates
    Unsubscribe(StateArgs),
    /// Add a to-do item for a merge request
    Todo(StateArgs),
    /// List users eligible to approve a merge request
    Approvers(StateArgs),
    /// List issues linked to a merge request
    Issues(StateArgs),
    /// Merge a merge request
    Merge(MergeArgs),
    /// Manage merge request discussions and notes
    Note(NoteArgs),
}
#[derive(Debug, Args)]
#[command(group(
    clap::ArgGroup::new("description_source")
        .args(["description", "description_file"])
        .multiple(false)
))]
pub struct CreateArgs {
    /// Merge request title
    #[arg(long, short = 't', value_parser = common::parse_non_blank)]
    pub title: String,
    /// Merge request description
    #[arg(long, short = 'd')]
    pub description: Option<String>,
    /// Read description from a file; `-` reads standard input
    #[arg(long = "description-file", value_name = "FILE")]
    pub description_file: Option<String>,
    /// Source branch; defaults to the current branch when available
    #[arg(long = "source-branch", short = 's', value_parser = common::parse_non_blank)]
    pub source_branch: Option<String>,
    /// Target branch
    #[arg(long = "target-branch", short = 'b', value_parser = common::parse_non_blank)]
    pub target_branch: Option<String>,
    /// Select a head repository
    #[arg(long = "head", short = 'H', value_parser = common::parse_non_blank)]
    pub head: Option<String>,
    /// Mark the merge request as draft
    #[arg(long)]
    pub draft: bool,
    /// Mark the merge request as work in progress
    #[arg(long)]
    pub wip: bool,
    /// Add an assignee; may be repeated or comma-separated
    #[arg(long = "assignee", short = 'a', value_parser = common::parse_non_blank)]
    pub assignees: Vec<String>,
    /// Add a label; may be repeated or comma-separated
    #[arg(long = "label", short = 'l', value_parser = common::parse_non_blank)]
    pub labels: Vec<String>,
    /// Request a review from a username; may be repeated
    #[arg(long = "reviewer", value_parser = common::parse_non_blank)]
    pub reviewers: Vec<String>,
    /// Assign a milestone by title or global ID
    #[arg(long = "milestone", short = 'm', value_parser = common::parse_non_blank)]
    pub milestone: Option<String>,
    /// Create the merge request for an issue IID
    #[arg(long = "related-issue", short = 'i', value_parser = clap::value_parser!(u64).range(1..))]
    pub related_issue: Option<u64>,
    /// Copy labels from the related issue
    #[arg(long = "copy-issue-labels")]
    pub copy_issue_labels: bool,
    /// Allow commits from other members
    #[arg(long = "allow-collaboration", action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true")]
    pub allow_collaboration: Option<bool>,
    /// Enable auto-merge when checks pass
    #[arg(long = "auto-merge")]
    pub auto_merge: bool,
    /// Create the source branch when it does not exist
    #[arg(long = "create-source-branch")]
    pub create_source_branch: bool,
    /// Remove the source branch after merge
    #[arg(long = "remove-source-branch", action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true")]
    pub remove_source_branch: Option<bool>,
    /// Squash commits before merge
    #[arg(long = "squash-before-merge", action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true")]
    pub squash_before_merge: Option<bool>,
    /// Add a DCO sign-off to the description
    #[arg(long)]
    pub signoff: bool,
    /// Push the source branch before creating the merge request
    #[arg(long)]
    pub push: bool,
    /// Populate title and description from commits
    #[arg(long, requires = "fill_commit_body")]
    pub fill: bool,
    /// Include commit bodies; requires --fill
    #[arg(long = "fill-commit-body", requires = "fill")]
    pub fill_commit_body: bool,
    /// Read a local merge-request template
    #[arg(long, value_parser = common::parse_non_blank)]
    pub template: Option<String>,
}
#[derive(Debug, Args)]
pub struct ListArgs {
    /// Include all states
    #[arg(long)]
    pub all: bool,
    /// List closed merge requests
    #[arg(long)]
    pub closed: bool,
    /// List merged merge requests
    #[arg(long)]
    pub merged: bool,
    /// List draft merge requests
    #[arg(long)]
    pub draft: bool,
    /// Exclude draft merge requests
    #[arg(long = "not-draft")]
    pub not_draft: bool,
    /// Filter by label; may be repeated
    #[arg(long = "label", value_parser = common::parse_non_blank)]
    pub labels: Vec<String>,
    /// Exclude merge requests with labels; may be repeated
    #[arg(long = "not-label", value_parser = common::parse_non_blank)]
    pub not_labels: Vec<String>,
    /// Filter by assignee
    #[arg(long, value_parser = common::parse_non_blank)]
    pub assignee: Option<String>,
    /// Filter by author
    #[arg(long, value_parser = common::parse_non_blank)]
    pub author: Option<String>,
    /// Filter by reviewer
    #[arg(long, value_parser = common::parse_non_blank)]
    pub reviewer: Option<String>,
    /// Filter by milestone ID
    #[arg(long, value_parser = common::parse_non_blank)]
    pub milestone: Option<String>,
    /// Filter by source branch
    #[arg(long = "source-branch", value_parser = common::parse_non_blank)]
    pub source_branch: Option<String>,
    /// Filter by target branch
    #[arg(long = "target-branch", value_parser = common::parse_non_blank)]
    pub target_branch: Option<String>,
    /// Search merge requests
    #[arg(long, value_parser = common::parse_non_blank)]
    pub search: Option<String>,
    /// Filter by creation date (ISO 8601)
    #[arg(long, value_parser = common::parse_non_blank)]
    pub created_after: Option<String>,
    /// Filter by creation date (ISO 8601)
    #[arg(long, value_parser = common::parse_non_blank)]
    pub created_before: Option<String>,
    /// Filter by deployment date (ISO 8601)
    #[arg(long, value_parser = common::parse_non_blank)]
    pub deployed_after: Option<String>,
    /// Filter by deployment date (ISO 8601)
    #[arg(long, value_parser = common::parse_non_blank)]
    pub deployed_before: Option<String>,
    /// Filter by deployment environment
    #[arg(long, value_parser = common::parse_non_blank)]
    pub environment: Option<String>,
    /// Order results by a native GitLab field
    #[arg(long, value_parser = ["created_at", "updated_at", "merged_at", "title", "priority", "label_priority", "milestone_due", "popularity"])]
    pub order: Option<String>,
    /// Sort order direction
    #[arg(long, value_parser = ["asc", "desc"])]
    pub sort: Option<String>,
    /// Maximum number of merge requests to return
    #[arg(
        long,
        default_value_t = 30,
        value_parser = common::parse_per_page
    )]
    pub per_page: usize,
    /// Page number
    #[arg(long, default_value_t = 1, value_parser = parse_page)]
    pub page: usize,
}

#[derive(Debug, Args)]
pub struct ViewArgs {
    /// Merge request IID
    pub number: common::IssueNumber,
}

#[derive(Debug, Args)]
pub struct StateArgs {
    /// Merge request IID
    pub number: common::IssueNumber,
}

#[derive(Debug, Args)]
pub struct DiffArgs {
    /// Merge request IID
    pub number: common::IssueNumber,
    /// Request raw diff format
    #[arg(long)]
    pub raw: bool,
}

#[derive(Debug, Args)]
pub struct CheckoutArgs {
    /// Merge request IID
    pub number: common::IssueNumber,
}

#[derive(Debug, Args)]
pub struct MergeArgs {
    /// Merge request IID
    pub number: common::IssueNumber,
    /// Set auto-merge explicitly; omit to use glab's default behavior
    #[arg(long = "auto-merge", action = clap::ArgAction::Set, num_args = 0..=1, default_missing_value = "true")]
    pub auto_merge: Option<bool>,
    /// Merge commit message
    #[arg(long, short, value_parser = common::parse_non_blank)]
    pub message: Option<String>,
    /// Rebase commits onto the target branch
    #[arg(long, short)]
    pub rebase: bool,
    /// Set source-branch removal explicitly; omit to use project defaults
    #[arg(
        long = "remove-source-branch",
        short = 'd',
        action = clap::ArgAction::Set,
        num_args = 0..=1,
        default_missing_value = "true"
    )]
    pub remove_source_branch: Option<bool>,
    /// Merge only if the source branch HEAD matches this SHA
    #[arg(long, value_parser = common::parse_non_blank)]
    pub sha: Option<String>,
    /// Squash commits when merging
    #[arg(long, short)]
    pub squash: bool,
    /// Squash commit message
    #[arg(long = "squash-message", value_parser = common::parse_non_blank)]
    pub squash_message: Option<String>,
    /// Skip confirmation prompt
    #[arg(long, short)]
    pub yes: bool,
}

#[derive(Debug, Args)]
#[command(disable_help_subcommand = true)]
pub struct NoteArgs {
    #[command(subcommand)]
    pub action: NoteAction,
}

#[derive(Debug, Subcommand)]
pub enum NoteAction {
    /// Create a discussion or comment
    Create(NoteCreateArgs),
    /// List discussions
    List(NoteListArgs),
    /// Update a note body
    Update(NoteUpdateArgs),
    /// Resolve a discussion
    Resolve(NoteDiscussionArgs),
    /// Reopen a resolved discussion
    Reopen(NoteDiscussionArgs),
}
#[derive(Debug, Args)]
pub struct NoteListArgs {
    /// Merge request IID
    pub number: common::IssueNumber,
    /// Filter resolution state
    #[arg(long, value_parser = ["all", "resolved", "unresolved"])]
    pub state: Option<String>,
    /// Filter note type
    #[arg(long = "type", short = 't', value_parser = ["all", "general", "diff", "system"])]
    pub note_type: Option<String>,
    /// Filter diff notes to a file path
    #[arg(long, value_parser = common::parse_non_blank)]
    pub file: Option<String>,
}
#[derive(Debug, Args)]
pub struct NoteUpdateArgs {
    /// Merge request IID
    pub number: common::IssueNumber,
    /// Numeric note ID
    #[arg(value_parser = parse_positive)]
    pub note_id: u64,
    /// Replacement body; omitted body is read from standard input
    #[arg(long, short, conflicts_with = "body_file", value_parser = common::parse_non_blank)]
    pub message: Option<String>,
    /// Read replacement body from a file; `-` reads standard input
    #[arg(long = "body-file", value_name = "FILE", conflicts_with = "message")]
    pub body_file: Option<String>,
}
#[derive(Debug, Args)]
#[command(
    group(clap::ArgGroup::new("discussion_target").args(["file", "reply", "unique"])),
    group(clap::ArgGroup::new("diff_side").args(["line", "old_line"]))
)]
pub struct NoteCreateArgs {
    /// Merge request IID
    pub number: common::IssueNumber,
    /// Comment or discussion body
    #[arg(long, short, value_parser = common::parse_non_blank)]
    pub message: String,
    /// File path for a diff comment
    #[arg(long, value_parser = common::parse_non_blank)]
    pub file: Option<String>,
    /// New-side line number or range (for example 10:15)
    #[arg(long, requires = "file", value_parser = parse_line_range)]
    pub line: Option<String>,
    /// Old-side line number for a removed line
    #[arg(long, requires = "file", value_parser = parse_positive)]
    pub old_line: Option<u64>,
    /// Reply to a discussion ID or unique prefix of at least 8 characters
    #[arg(long, value_parser = parse_discussion_id)]
    pub reply: Option<String>,
    /// Set whether the discussion blocks merge until resolved
    #[arg(
        long,
        action = clap::ArgAction::Set,
        num_args = 0..=1,
        default_missing_value = "true"
    )]
    pub resolvable: Option<bool>,
    /// Skip creating a duplicate note with the same body
    #[arg(long)]
    pub unique: bool,
}

#[derive(Debug, Args)]
pub struct NoteDiscussionArgs {
    /// Discussion ID, prefix, or numeric note ID
    #[arg(value_parser = parse_note_identifier)]
    pub discussion_id: String,
    /// Merge request IID
    pub number: common::IssueNumber,
}

fn parse_note_identifier(value: &str) -> Result<String, &'static str> {
    if value.parse::<u64>().is_ok_and(|number| number > 0) {
        return Ok(value.to_owned());
    }
    parse_discussion_id(value)
}
fn parse_positive(value: &str) -> Result<u64, &'static str> {
    value
        .parse::<u64>()
        .ok()
        .filter(|number| *number > 0)
        .ok_or("value must be a positive integer")
}

fn parse_line_range(value: &str) -> Result<String, &'static str> {
    let mut lines = value.split(':');
    let start = lines
        .next()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|number| *number > 0)
        .ok_or("line must be a positive line number or range")?;
    if let Some(end) = lines.next() {
        let end = end
            .parse::<u64>()
            .ok()
            .filter(|number| *number >= start)
            .ok_or("line range must have a positive end not smaller than its start")?;
        if lines.next().is_some() {
            return Err("line must be a line number or one range");
        }
        Ok(format!("{start}:{end}"))
    } else {
        Ok(start.to_string())
    }
}

fn parse_discussion_id(value: &str) -> Result<String, &'static str> {
    if value.len() < 8 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("discussion ID must be a hexadecimal ID or prefix of at least 8 characters");
    }
    Ok(value.to_owned())
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
