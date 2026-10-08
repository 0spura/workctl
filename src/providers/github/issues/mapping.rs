use serde::Deserialize;
use serde_json::Value;

use crate::domain::{AppError, Issue, IssueState, IssueSummary, RelatedIssue};

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

#[derive(Deserialize)]
struct ApiRelatedIssue {
    number: u64,
    title: String,
    state: String,
    url: String,
}

impl ApiRelatedIssue {
    fn normalize(self) -> Result<RelatedIssue, AppError> {
        if self.number == 0 || self.title.trim().is_empty() || self.url.is_empty() {
            return Err(AppError::provider_response());
        }
        Ok(RelatedIssue {
            number: self.number,
            title: self.title,
            state: parse_state(&self.state)?,
            url: self.url,
        })
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiConnection {
    nodes: Vec<ApiRelatedIssue>,
    total_count: usize,
    page_info: PageInfo,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageInfo {
    has_next_page: bool,
    end_cursor: Option<String>,
}

pub(super) struct RelationPage {
    pub nodes: Vec<RelatedIssue>,
    pub total_count: usize,
    pub next_cursor: Option<String>,
}

impl ApiConnection {
    fn normalize(self) -> Result<RelationPage, AppError> {
        if self.nodes.len() > 100
            || (self.page_info.has_next_page
                && (self.nodes.is_empty()
                    || self
                        .page_info
                        .end_cursor
                        .as_ref()
                        .is_none_or(|cursor| cursor.is_empty())))
        {
            return Err(AppError::provider_response());
        }
        Ok(RelationPage {
            nodes: self
                .nodes
                .into_iter()
                .map(ApiRelatedIssue::normalize)
                .collect::<Result<_, _>>()?,
            total_count: self.total_count,
            next_cursor: self
                .page_info
                .has_next_page
                .then_some(self.page_info.end_cursor)
                .flatten(),
        })
    }
}

#[derive(Deserialize)]
struct ApiIssueType {
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiRelationships {
    #[serde(deserialize_with = "Option::deserialize")]
    issue_type: Option<ApiIssueType>,
    #[serde(deserialize_with = "Option::deserialize")]
    parent: Option<ApiRelatedIssue>,
    sub_issues: ApiConnection,
}

pub(super) struct Relationships {
    pub issue_type: Option<String>,
    pub parent: Option<RelatedIssue>,
    pub sub_issues: RelationPage,
}

pub(super) fn relationships(bytes: &[u8]) -> Result<Relationships, AppError> {
    let response: ApiRelationships =
        serde_json::from_value(graphql_issue(bytes)?).map_err(|_| AppError::provider_response())?;
    let issue_type = response.issue_type.map(|issue_type| issue_type.name);
    if issue_type
        .as_ref()
        .is_some_and(|name| name.trim().is_empty())
    {
        return Err(AppError::provider_response());
    }
    Ok(Relationships {
        issue_type,
        parent: response
            .parent
            .map(ApiRelatedIssue::normalize)
            .transpose()?,
        sub_issues: response.sub_issues.normalize()?,
    })
}

pub(super) fn relation_page(bytes: &[u8]) -> Result<RelationPage, AppError> {
    let mut issue = graphql_issue(bytes)?;
    let connection: ApiConnection = serde_json::from_value(
        issue
            .as_object_mut()
            .and_then(|issue| issue.remove("relations"))
            .ok_or_else(AppError::provider_response)?,
    )
    .map_err(|_| AppError::provider_response())?;
    connection.normalize()
}

fn graphql_issue(bytes: &[u8]) -> Result<Value, AppError> {
    let mut response: Value =
        serde_json::from_slice(bytes).map_err(|_| AppError::provider_response())?;
    if response.get("errors").is_some_and(|errors| {
        !errors.is_null() && errors.as_array().is_none_or(|errors| !errors.is_empty())
    }) {
        return Err(AppError::provider_response());
    }
    response
        .get_mut("data")
        .and_then(|data| data.get_mut("repository"))
        .and_then(|repository| repository.get_mut("issue"))
        .filter(|issue| issue.is_object())
        .map(Value::take)
        .ok_or_else(AppError::provider_response)
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
