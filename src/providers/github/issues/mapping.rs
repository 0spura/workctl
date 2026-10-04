use serde::Deserialize;
use serde_json::Value;

use crate::domain::{AppError, Issue, IssueState, IssueSummary};

#[derive(Deserialize)]
struct ApiIssue {
    number: u64,
    title: String,
    body: Value,
    state: String,
    html_url: String,
    created_at: String,
    updated_at: String,
    pull_request: Option<Value>,
}

#[derive(Deserialize)]
struct CliIssueSummary {
    number: u64,
    title: String,
    state: String,
    url: String,
    #[serde(rename = "updatedAt")]
    updated_at: String,
}

pub fn issue(bytes: &[u8]) -> Result<Issue, AppError> {
    let response: ApiIssue =
        serde_json::from_slice(bytes).map_err(|_| AppError::provider_response())?;
    if response.pull_request.is_some() {
        return Err(AppError::not_issue());
    }
    if response.number == 0
        || response.title.trim().is_empty()
        || response.html_url.is_empty()
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
    Ok(Issue {
        number: response.number,
        title: response.title,
        body,
        state: parse_state(&response.state)?,
        url: response.html_url,
        created_at: response.created_at,
        updated_at: response.updated_at,
    })
}

pub fn issue_summaries(bytes: &[u8]) -> Result<Vec<IssueSummary>, AppError> {
    let responses: Vec<CliIssueSummary> =
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
            Ok(IssueSummary {
                number: response.number,
                title: response.title,
                state: parse_state(&response.state)?,
                url: response.url,
                updated_at: response.updated_at,
            })
        })
        .collect()
}

fn parse_state(value: &str) -> Result<IssueState, AppError> {
    if value.eq_ignore_ascii_case("open") {
        Ok(IssueState::Open)
    } else if value.eq_ignore_ascii_case("closed") {
        Ok(IssueState::Closed)
    } else {
        Err(AppError::provider_response())
    }
}

#[cfg(test)]
mod tests {
    use super::issue;

    #[test]
    fn normalizes_null_body_and_provider_state() {
        let response = br#"{"number":4,"title":"Title","body":null,"state":"OPEN","html_url":"https://github.com/owner/repo/issues/4","created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-02T00:00:00Z"}"#;
        let issue = issue(response).expect("valid GitHub issue");
        assert_eq!(issue.body, "");
        assert_eq!(
            serde_json::to_value(issue).expect("serialize issue")["state"],
            "open"
        );
    }

    #[test]
    fn rejects_missing_required_fields_and_unknown_states() {
        let missing_body = br#"{"number":4,"title":"Title","state":"OPEN","html_url":"https://github.com/owner/repo/issues/4","created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-02T00:00:00Z"}"#;
        assert_eq!(issue(missing_body).unwrap_err().code, "provider_response");

        let unknown_state = br#"{"number":4,"title":"Title","body":"","state":"pending","html_url":"https://github.com/owner/repo/issues/4","created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-02T00:00:00Z"}"#;
        assert_eq!(issue(unknown_state).unwrap_err().code, "provider_response");
    }
}
