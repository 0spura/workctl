mod mapping;
mod read;
mod write;

use crate::domain::{AppError, Issue, IssueSummary};

/// Fields accepted by the native `glab issue create` command.
#[derive(Debug)]
pub struct GitLabIssueCreate {
    pub title: String,
    pub description: String,
    pub labels: Vec<String>,
    pub assignees: Vec<String>,
    pub milestone: Option<String>,
    pub confidential: bool,
    pub weight: Option<u64>,
    pub due_date: Option<String>,
    pub epic: Option<u64>,
    pub linked_issues: Vec<String>,
    pub link_type: Option<String>,
    pub linked_mr: Option<u64>,
    pub time_estimate: Option<String>,
    pub time_spent: Option<String>,
    pub template: Option<String>,
}

/// Fields accepted by the native `glab issue update` command.
#[derive(Debug)]
pub struct GitLabIssueUpdate {
    pub title: Option<String>,
    pub description: Option<String>,
    pub labels_add: Vec<String>,
    pub labels_remove: Vec<String>,
    pub assignees: Vec<String>,
    pub unassign: bool,
    /// `Some("")` and `Some("0")` both clear the current milestone.
    pub milestone: Option<String>,
    pub confidential: Option<bool>,
    pub weight: Option<u64>,
    pub due_date: Option<String>,
}

/// GitLab's issue-list filters are intentionally provider-specific rather than part of the
/// GitHub-shaped shared issue query.
#[derive(Debug)]
pub struct GitLabIssueQuery {
    pub all: bool,
    pub closed: bool,
    pub labels: Vec<String>,
    pub assignee: Option<String>,
    pub author: Option<String>,
    pub milestone: Option<String>,
    pub search: Option<String>,
    pub in_fields: Option<String>,
    pub confidential: bool,
    pub issue_type: Option<String>,
    pub iteration: Option<String>,
    pub not_assignees: Vec<String>,
    pub not_authors: Vec<String>,
    pub not_labels: Vec<String>,
    pub order: Option<String>,
    pub sort: Option<String>,
    pub page: usize,
    pub per_page: usize,
}
/// GitLab issue operations. Native write fields stay provider-owned; successful writes are mapped
/// back to the shared `Issue` result through `glab issue view --output json`.
pub struct GitLabIssues {
    repo_url: String,
    host: String,
}

impl GitLabIssues {
    pub fn new(repo_url: String) -> Result<Self, AppError> {
        let url = reqwest::Url::parse(&repo_url)
            .map_err(|_| AppError::invalid_input("GitLab repository URL is invalid"))?;
        let host = url
            .host_str()
            .ok_or_else(|| AppError::invalid_input("GitLab repository URL is invalid"))?;
        let host = match url.port() {
            Some(port) => format!("{host}:{port}"),
            None => host.to_owned(),
        };
        Ok(Self { repo_url, host })
    }

    pub fn authenticate(&self) -> Result<(), AppError> {
        super::authenticate(&self.host)
    }

    pub fn list(&self, query: &GitLabIssueQuery) -> Result<Vec<IssueSummary>, AppError> {
        read::list(self, query)
    }

    pub fn show(&self, number: u64) -> Result<Issue, AppError> {
        read::show(self, number)
    }

    pub fn labels(&self) -> Result<Vec<crate::domain::RepositoryLabel>, AppError> {
        read::labels(self)
    }

    pub fn create(&self, issue: &GitLabIssueCreate) -> Result<Issue, AppError> {
        write::create(self, issue)
    }

    pub fn set_state(&self, number: u64, closed: bool) -> Result<(), AppError> {
        let command = if closed { "close" } else { "reopen" };
        let args = [
            "issue".to_owned(),
            command.to_owned(),
            number.to_string(),
            "--repo".to_owned(),
            self.repo_argument(),
        ];
        self.run_glab_mutation(&args, None)?;
        Ok(())
    }

    pub fn set_subscription(&self, number: u64, subscribed: bool) -> Result<(), AppError> {
        let command = if subscribed {
            "subscribe"
        } else {
            "unsubscribe"
        };
        let args = [
            "issue".to_owned(),
            command.to_owned(),
            number.to_string(),
            "--repo".to_owned(),
            self.repo_argument(),
        ];
        self.run_glab_mutation(&args, None)?;
        Ok(())
    }

    pub fn update(&self, number: u64, patch: &GitLabIssueUpdate) -> Result<Issue, AppError> {
        write::update(self, number, patch)
    }

    pub(super) fn run_glab(&self, args: &[String]) -> Result<Vec<u8>, AppError> {
        super::run_glab(args)
    }

    pub(super) fn run_glab_mutation(
        &self,
        args: &[String],
        input: Option<Vec<u8>>,
    ) -> Result<Vec<u8>, AppError> {
        super::run_glab_mutation(args, input)
    }

    pub(super) fn repo_argument(&self) -> String {
        super::repo_argument(&self.repo_url)
    }

    /// Uses `glab api`'s documented JSON-body stdin mode because `glab issue create/update`
    /// expose descriptions only as argv strings, not a supported stdin flag.
    pub(super) fn set_description(&self, number: u64, description: &str) -> Result<(), AppError> {
        let project_path = self
            .repo_url
            .strip_prefix("https://")
            .and_then(|url| url.split_once('/').map(|(_, path)| path))
            .ok_or_else(|| AppError::invalid_input("GitLab repository URL is invalid"))?;
        let encoded_project = project_path.replace('/', "%2F");
        let endpoint = format!("projects/{encoded_project}/issues/{number}");
        let args = [
            "api".to_owned(),
            "--method".to_owned(),
            "PUT".to_owned(),
            endpoint,
            "--hostname".to_owned(),
            self.host.clone(),
            "--input".to_owned(),
            "-".to_owned(),
        ];
        let body = serde_json::to_vec(&serde_json::json!({ "description": description }))
            .map_err(|_| AppError::provider_response())?;
        super::run_glab_mutation(&args, Some(body))?;
        Ok(())
    }

    /// Posts a GitLab issue note using glab's stdin-backed API request mode.
    pub fn add_note(&self, number: u64, message: &str) -> Result<(), AppError> {
        let project_path = self
            .repo_url
            .strip_prefix("https://")
            .and_then(|url| url.split_once('/').map(|(_, path)| path))
            .ok_or_else(|| AppError::invalid_input("GitLab repository URL is invalid"))?;
        let encoded_project = project_path.replace('/', "%2F");
        let endpoint = format!("projects/{encoded_project}/issues/{number}/notes");
        let args = [
            "api".to_owned(),
            "--method".to_owned(),
            "POST".to_owned(),
            endpoint,
            "--hostname".to_owned(),
            self.host.clone(),
            "--input".to_owned(),
            "-".to_owned(),
        ];
        let body = serde_json::to_vec(&serde_json::json!({ "body": message }))
            .map_err(|_| AppError::provider_response())?;
        super::run_glab_mutation(&args, Some(body))?;
        Ok(())
    }
}
