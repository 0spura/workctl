use crate::domain::RepositoryLabel;
use serde::Deserialize;

use crate::domain::{AppError, Issue, IssueState, IssueSummary};

/// One issue as `glab issue list --output json` and `glab issue view --output json` print it:
/// the GitLab API resource, with `iid` instead of a repository-scoped `number` and `description`
/// where GitHub says `body`.
#[derive(Deserialize)]
struct ApiIssue {
    iid: u64,
    title: String,
    description: Option<String>,
    state: String,
    web_url: String,
    created_at: String,
    updated_at: String,
}

#[derive(Deserialize)]
struct ApiLabel {
    name: String,
    description: Option<String>,
}

pub fn labels(bytes: &[u8]) -> Result<Vec<RepositoryLabel>, AppError> {
    let responses: Vec<ApiLabel> =
        serde_json::from_slice(bytes).map_err(|_| AppError::provider_response())?;
    let mut names = std::collections::HashSet::with_capacity(responses.len());
    responses
        .into_iter()
        .map(|label| {
            if label.name.trim().is_empty() || !names.insert(label.name.clone()) {
                return Err(AppError::provider_response());
            }
            Ok(RepositoryLabel {
                name: label.name,
                description: label.description.filter(|value| !value.trim().is_empty()),
            })
        })
        .collect()
}

pub fn issue(bytes: &[u8]) -> Result<Issue, AppError> {
    let response: ApiIssue =
        serde_json::from_slice(bytes).map_err(|_| AppError::provider_response())?;
    let state = parse_state(&response.state)?;
    validate(&response)?;
    Ok(Issue {
        number: response.iid,
        title: response.title,
        body: response.description.unwrap_or_default(),
        state,
        url: response.web_url,
        created_at: response.created_at,
        updated_at: response.updated_at,
    })
}

pub fn issue_summaries(bytes: &[u8]) -> Result<Vec<IssueSummary>, AppError> {
    let responses: Vec<ApiIssue> =
        serde_json::from_slice(bytes).map_err(|_| AppError::provider_response())?;
    responses.into_iter().map(summary).collect()
}

fn summary(response: ApiIssue) -> Result<IssueSummary, AppError> {
    let state = parse_state(&response.state)?;
    validate(&response)?;
    Ok(IssueSummary {
        number: response.iid,
        title: response.title,
        state,
        url: response.web_url,
        updated_at: response.updated_at,
    })
}

fn validate(response: &ApiIssue) -> Result<(), AppError> {
    if response.iid == 0
        || response.title.trim().is_empty()
        || response.web_url.is_empty()
        || response.created_at.is_empty()
        || response.updated_at.is_empty()
    {
        return Err(AppError::provider_response());
    }
    Ok(())
}

/// GitLab spells the open state `opened`; `workctl` reports the shared `open`.
fn parse_state(value: &str) -> Result<IssueState, AppError> {
    match value {
        "opened" => Ok(IssueState::Open),
        "closed" => Ok(IssueState::Closed),
        _ => Err(AppError::provider_response()),
    }
}

#[cfg(test)]
mod tests {
    use super::{issue, issue_summaries, labels, parse_state};

    const OPEN_ISSUE: &str = r#"{
        "iid": 12,
        "title": "Crash on startup",
        "description": "Steps to reproduce",
        "state": "opened",
        "web_url": "https://gitlab.com/g/p/-/issues/12",
        "created_at": "2026-01-02T03:04:05.000Z",
        "updated_at": "2026-01-03T04:05:06.000Z"
    }"#;

    #[test]
    fn maps_a_gitlab_issue_onto_the_shared_shape() {
        let mapped = issue(OPEN_ISSUE.as_bytes()).expect("valid issue");
        assert_eq!(mapped.number, 12);
        assert_eq!(mapped.body, "Steps to reproduce");
        assert_eq!(mapped.url, "https://gitlab.com/g/p/-/issues/12");
    }

    #[test]
    fn treats_a_missing_description_as_an_empty_body() {
        let payload = OPEN_ISSUE.replace(
            r#""description": "Steps to reproduce""#,
            r#""description": null"#,
        );
        assert_eq!(issue(payload.as_bytes()).expect("valid issue").body, "");
    }

    #[test]
    fn rejects_unknown_states_and_incomplete_records() {
        assert_eq!(parse_state("locked").unwrap_err().code, "provider_response");
        let payload = OPEN_ISSUE.replace(r#""iid": 12"#, r#""iid": 0"#);
        assert_eq!(
            issue(payload.as_bytes()).unwrap_err().code,
            "provider_response"
        );
        assert_eq!(
            issue_summaries(b"{}").unwrap_err().code,
            "provider_response"
        );
    }

    #[test]
    fn maps_a_summary_page_and_reports_the_state() {
        let payload = format!("[{OPEN_ISSUE}]");
        let summaries = issue_summaries(payload.as_bytes()).expect("valid page");
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].number, 12);
        assert!(matches!(
            summaries[0].state,
            crate::domain::IssueState::Open
        ));
    }

    #[test]
    fn maps_native_label_catalog_and_rejects_duplicate_names() {
        let catalog = labels(
            br#"[{"name":"bug","description":"Broken behavior"},{"name":"docs","description":null}]"#,
        )
        .expect("valid labels");
        assert_eq!(catalog[0].name, "bug");
        assert_eq!(catalog[0].description.as_deref(), Some("Broken behavior"));
        assert_eq!(catalog[1].description, None);
        assert_eq!(
            labels(br#"[{"name":"bug"},{"name":"bug"}]"#)
                .unwrap_err()
                .code,
            "provider_response"
        );
    }
}
