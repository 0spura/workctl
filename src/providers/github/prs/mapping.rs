use serde::Deserialize;
use serde_json::Value;

use crate::domain::{
    AppError, CheckRun, PullRequest, PullRequestState, PullRequestStatus, PullRequestSummary,
};

#[derive(Deserialize)]
struct CliUser {
    login: String,
}

#[derive(Deserialize)]
struct CliLabel {
    name: String,
}

#[derive(Deserialize)]
struct CliPullRequest {
    number: u64,
    title: String,
    body: Value,
    state: String,
    #[serde(rename = "isDraft")]
    is_draft: bool,
    url: String,
    #[serde(rename = "baseRefName")]
    base_ref_name: String,
    #[serde(rename = "headRefName")]
    head_ref_name: String,
    author: Option<CliUser>,
    #[serde(rename = "createdAt")]
    created_at: String,
    #[serde(rename = "updatedAt")]
    updated_at: String,
    #[serde(rename = "mergedAt")]
    merged_at: Option<String>,
    mergeable: Option<String>,
    #[serde(rename = "reviewDecision")]
    review_decision: Option<String>,
    #[serde(default)]
    labels: Vec<CliLabel>,
    #[serde(default)]
    assignees: Vec<CliUser>,
}

#[derive(Deserialize)]
struct CliPullRequestSummary {
    number: u64,
    title: String,
    state: String,
    #[serde(rename = "isDraft")]
    is_draft: bool,
    url: String,
    #[serde(rename = "baseRefName")]
    base_ref_name: String,
    #[serde(rename = "headRefName")]
    head_ref_name: String,
    #[serde(rename = "updatedAt")]
    updated_at: String,
}

#[derive(Deserialize)]
struct CliPullRequestStatus {
    number: u64,
    title: String,
    state: String,
    #[serde(rename = "isDraft")]
    is_draft: bool,
    url: String,
    #[serde(rename = "baseRefName")]
    base_ref_name: String,
    #[serde(rename = "headRefName")]
    head_ref_name: String,
    mergeable: Option<String>,
    #[serde(rename = "reviewDecision")]
    review_decision: Option<String>,
}

#[derive(Deserialize)]
struct CliCheckRun {
    name: String,
    state: String,
    bucket: Option<String>,
    description: Option<String>,
    link: Option<String>,
    workflow: Option<String>,
}

#[derive(Deserialize)]
struct CliHeadRef {
    #[serde(rename = "headRefOid")]
    head_ref_oid: String,
}

pub fn pull_request(bytes: &[u8]) -> Result<PullRequest, AppError> {
    let response: CliPullRequest =
        serde_json::from_slice(bytes).map_err(|_| AppError::provider_response())?;
    if response.number == 0
        || response.title.trim().is_empty()
        || response.url.is_empty()
        || response.created_at.is_empty()
        || response.updated_at.is_empty()
    {
        return Err(AppError::provider_response());
    }
    let body = match response.body {
        Value::Null => String::new(),
        Value::String(body) => body,
        _ => return Err(AppError::provider_response()),
    };
    Ok(PullRequest {
        number: response.number,
        title: response.title,
        body,
        state: parse_state(&response.state)?,
        draft: response.is_draft,
        url: response.url,
        base_ref: response.base_ref_name,
        head_ref: response.head_ref_name,
        author: response
            .author
            .map(|author| author.login)
            .unwrap_or_default(),
        created_at: response.created_at,
        updated_at: response.updated_at,
        merged_at: non_empty(response.merged_at),
        mergeable: non_empty(response.mergeable).map(|value| value.to_ascii_lowercase()),
        review_decision: non_empty(response.review_decision)
            .map(|value| value.to_ascii_lowercase()),
        labels: response
            .labels
            .into_iter()
            .map(|label| label.name)
            .collect(),
        assignees: response
            .assignees
            .into_iter()
            .map(|assignee| assignee.login)
            .collect(),
    })
}

pub fn pull_request_status(bytes: &[u8]) -> Result<PullRequestStatus, AppError> {
    let response: CliPullRequestStatus =
        serde_json::from_slice(bytes).map_err(|_| AppError::provider_response())?;
    if response.number == 0
        || response.title.trim().is_empty()
        || response.url.is_empty()
        || response.base_ref_name.is_empty()
        || response.head_ref_name.is_empty()
    {
        return Err(AppError::provider_response());
    }
    Ok(PullRequestStatus {
        number: response.number,
        title: response.title,
        state: parse_state(&response.state)?,
        draft: response.is_draft,
        url: response.url,
        base_ref: response.base_ref_name,
        head_ref: response.head_ref_name,
        mergeable: non_empty(response.mergeable).map(|value| value.to_ascii_lowercase()),
        review_decision: non_empty(response.review_decision)
            .map(|value| value.to_ascii_lowercase()),
        required_checks: Vec::new(),
    })
}

/// Reads the head commit OID from `gh pr view --json headRefOid`.
pub fn head_sha(bytes: &[u8]) -> Result<String, AppError> {
    let response: CliHeadRef =
        serde_json::from_slice(bytes).map_err(|_| AppError::provider_response())?;
    if response.head_ref_oid.trim().is_empty() {
        return Err(AppError::provider_response());
    }
    Ok(response.head_ref_oid)
}

pub fn pull_request_summaries(bytes: &[u8]) -> Result<Vec<PullRequestSummary>, AppError> {
    let responses: Vec<CliPullRequestSummary> =
        serde_json::from_slice(bytes).map_err(|_| AppError::provider_response())?;
    responses
        .into_iter()
        .map(|response| {
            if response.number == 0
                || response.title.trim().is_empty()
                || response.url.is_empty()
                || response.updated_at.is_empty()
            {
                return Err(AppError::provider_response());
            }
            Ok(PullRequestSummary {
                number: response.number,
                title: response.title,
                state: parse_state(&response.state)?,
                draft: response.is_draft,
                url: response.url,
                base_ref: response.base_ref_name,
                head_ref: response.head_ref_name,
                updated_at: response.updated_at,
            })
        })
        .collect()
}

pub fn check_runs(bytes: &[u8]) -> Result<Vec<CheckRun>, AppError> {
    let responses: Vec<CliCheckRun> =
        serde_json::from_slice(bytes).map_err(|_| AppError::provider_response())?;
    responses
        .into_iter()
        .map(|response| {
            if response.name.is_empty() {
                return Err(AppError::provider_response());
            }
            Ok(CheckRun {
                name: response.name,
                state: response.state.to_ascii_lowercase(),
                bucket: response.bucket,
                description: response.description,
                link: response.link,
                workflow: response.workflow,
            })
        })
        .collect()
}

/// Drops an optional provider field that the CLI reports as an empty string.
fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.is_empty())
}

fn parse_state(value: &str) -> Result<PullRequestState, AppError> {
    if value.eq_ignore_ascii_case("open") {
        Ok(PullRequestState::Open)
    } else if value.eq_ignore_ascii_case("closed") {
        Ok(PullRequestState::Closed)
    } else if value.eq_ignore_ascii_case("merged") {
        Ok(PullRequestState::Merged)
    } else {
        Err(AppError::provider_response())
    }
}

#[cfg(test)]
mod tests {
    use super::{check_runs, pull_request, pull_request_summaries};

    #[test]
    fn normalizes_state_and_extracts_nested_author_labels_and_assignees() {
        let response = br#"{"number":12,"title":"Add pull requests","body":null,"state":"OPEN","isDraft":true,"url":"https://github.com/owner/repo/pull/12","baseRefName":"main","headRefName":"feature","author":{"login":"octocat"},"createdAt":"2026-01-01T00:00:00Z","updatedAt":"2026-01-02T00:00:00Z","mergedAt":"","mergeable":"CONFLICTING","reviewDecision":"CHANGES_REQUESTED","labels":[{"name":"bug"},{"name":"help wanted"}],"assignees":[{"login":"octocat"},{"login":"hubot"}]}"#;
        let pull = pull_request(response).expect("valid GitHub pull request");
        assert_eq!(pull.body, "");
        assert!(pull.draft);
        assert_eq!(pull.author, "octocat");
        assert_eq!(pull.labels, ["bug", "help wanted"]);
        assert_eq!(pull.assignees, ["octocat", "hubot"]);
        assert_eq!(pull.merged_at, None);
        assert_eq!(pull.mergeable.as_deref(), Some("conflicting"));
        assert_eq!(pull.review_decision.as_deref(), Some("changes_requested"));
        let value = serde_json::to_value(&pull).expect("serialize pull request");
        assert_eq!(value["state"], "open");
    }

    #[test]
    fn normalizes_merged_state_and_empty_optional_fields() {
        let response = br#"{"number":13,"title":"Fix checks","body":"text","state":"MERGED","isDraft":false,"url":"https://github.com/owner/repo/pull/13","baseRefName":"main","headRefName":"fix","author":{"login":"octocat"},"createdAt":"2026-01-01T00:00:00Z","updatedAt":"2026-01-03T00:00:00Z","mergedAt":"2026-01-03T00:00:00Z","mergeable":"MERGEABLE","reviewDecision":"","labels":[],"assignees":[]}"#;
        let pull = pull_request(response).expect("valid GitHub pull request");
        assert_eq!(pull.merged_at.as_deref(), Some("2026-01-03T00:00:00Z"));
        assert_eq!(pull.mergeable.as_deref(), Some("mergeable"));
        assert_eq!(pull.review_decision, None);
        assert_eq!(
            serde_json::to_value(&pull).expect("serialize pull request")["state"],
            "merged"
        );
    }

    #[test]
    fn rejects_missing_required_fields_and_unknown_states() {
        let missing_url = br#"{"number":12,"title":"Title","body":"","state":"OPEN","isDraft":false,"baseRefName":"main","headRefName":"feature","author":{"login":"octocat"},"createdAt":"2026-01-01T00:00:00Z","updatedAt":"2026-01-02T00:00:00Z","mergedAt":"","mergeable":"UNKNOWN","reviewDecision":"","labels":[],"assignees":[]}"#;
        assert_eq!(
            pull_request(missing_url).unwrap_err().code,
            "provider_response"
        );

        let unknown_state = br#"{"number":12,"title":"Title","body":"","state":"PENDING","isDraft":false,"url":"https://github.com/owner/repo/pull/12","baseRefName":"main","headRefName":"feature","author":{"login":"octocat"},"createdAt":"2026-01-01T00:00:00Z","updatedAt":"2026-01-02T00:00:00Z","mergedAt":"","mergeable":"UNKNOWN","reviewDecision":"","labels":[],"assignees":[]}"#;
        assert_eq!(
            pull_request(unknown_state).unwrap_err().code,
            "provider_response"
        );
    }

    #[test]
    fn summarizes_lists_with_state_and_draft() {
        let response = br#"[{"number":8,"title":"First","state":"CLOSED","isDraft":false,"url":"https://github.com/owner/repo/pull/8","baseRefName":"main","headRefName":"first","updatedAt":"2026-01-01T00:00:00Z"}]"#;
        let summaries = pull_request_summaries(response).expect("valid summaries");
        assert_eq!(summaries.len(), 1);
        assert!(!summaries[0].draft);
        assert_eq!(
            serde_json::to_value(&summaries[0]).expect("serialize summary")["state"],
            "closed"
        );
    }

    #[test]
    fn lowercases_check_state_and_keeps_bucket() {
        let response = br#"[{"name":"build","state":"SUCCESS","bucket":"pass","description":null,"link":"https://example.test/build","workflow":"CI"}]"#;
        let checks = check_runs(response).expect("valid check runs");
        assert_eq!(checks[0].name, "build");
        assert_eq!(checks[0].state, "success");
        assert_eq!(checks[0].bucket.as_deref(), Some("pass"));
        assert_eq!(checks[0].workflow.as_deref(), Some("CI"));
        assert_eq!(checks[0].description, None);
    }
}
