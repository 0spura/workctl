use std::collections::HashSet;

use serde::Deserialize;

use crate::domain::{AppError, Issue, IssueSummary, RepositoryLabel};
use crate::providers::github::issues::{mapping, GitHubIssues};
use crate::providers::IssueQuery;
pub(super) fn list(
    provider: &GitHubIssues,
    query: &IssueQuery,
) -> Result<Vec<IssueSummary>, AppError> {
    let mut args = vec![
        "issue".to_owned(),
        "list".to_owned(),
        "--repo".to_owned(),
        provider.repo.clone(),
        "--state".to_owned(),
        query.state.clone(),
        "--limit".to_owned(),
        query.limit.to_string(),
        "--json".to_owned(),
        "number,title,state,url,updatedAt".to_owned(),
    ];
    for label in &query.labels {
        args.push("--label".to_owned());
        args.push(label.clone());
    }
    for (flag, value) in [
        ("--assignee", &query.assignee),
        ("--author", &query.author),
        ("--mention", &query.mention),
        ("--milestone", &query.milestone),
        ("--search", &query.search),
        ("--type", &query.issue_type),
    ] {
        if let Some(value) = value {
            args.push(flag.to_owned());
            args.push(value.clone());
        }
    }
    let output = provider.run_gh(&args, None)?;
    let mut issues = mapping::issue_summaries(&output)?;
    issues.truncate(query.limit);
    Ok(issues)
}

pub(super) fn show(provider: &GitHubIssues, number: u64) -> Result<Issue, AppError> {
    let endpoint = format!("repos/{}/issues/{number}", provider.repo);
    let args = ["api".to_owned(), endpoint];
    mapping::issue(&provider.run_gh(&args, None)?)
}

#[derive(Deserialize)]
struct CliLabel {
    name: String,
    description: Option<String>,
}

pub(super) fn labels(provider: &GitHubIssues) -> Result<Vec<RepositoryLabel>, AppError> {
    const MAX_LABELS: usize = 1_000;
    let args = [
        "label".to_owned(),
        "list".to_owned(),
        "--repo".to_owned(),
        provider.repo.clone(),
        "--limit".to_owned(),
        (MAX_LABELS + 1).to_string(),
        "--json".to_owned(),
        "name,description".to_owned(),
    ];
    parse_labels(&provider.run_gh(&args, None)?)
}

fn parse_labels(bytes: &[u8]) -> Result<Vec<RepositoryLabel>, AppError> {
    const MAX_LABELS: usize = 1_000;
    let labels: Vec<CliLabel> =
        serde_json::from_slice(bytes).map_err(|_| AppError::provider_response())?;
    if labels.len() > MAX_LABELS || labels.iter().any(|label| label.name.trim().is_empty()) {
        return Err(AppError::provider_response());
    }
    let mut names = HashSet::with_capacity(labels.len());
    let mut labels = labels
        .into_iter()
        .map(|label| RepositoryLabel {
            name: label.name,
            description: label.description.filter(|value| !value.trim().is_empty()),
        })
        .collect::<Vec<_>>();
    if labels
        .iter()
        .any(|label| !names.insert(label.name.as_str()))
    {
        return Err(AppError::provider_response());
    }
    labels.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(labels)
}

#[cfg(test)]
mod tests {
    use super::parse_labels;

    #[test]
    fn parses_descriptions_and_sorts_repository_labels() {
        let labels = parse_labels(
            br#"[{"name":"z-docs","description":null},{"name":"bug","description":"Broken behavior"}]"#,
        )
        .expect("valid label catalog");
        assert_eq!(labels[0].name, "bug");
        assert_eq!(labels[0].description.as_deref(), Some("Broken behavior"));
        assert_eq!(labels[1].name, "z-docs");
        assert_eq!(labels[1].description, None);
    }

    #[test]
    fn rejects_invalid_or_oversized_label_catalogs() {
        assert_eq!(
            parse_labels(br#"[{"name":" ","description":null}]"#)
                .expect_err("blank label")
                .code,
            "provider_response"
        );
        assert_eq!(
            parse_labels(
                br#"[{"name":"bug","description":null},{"name":"bug","description":null}]"#
            )
            .expect_err("duplicate label")
            .code,
            "provider_response"
        );
        let labels = (0..=1_000)
            .map(|index| serde_json::json!({"name": format!("label-{index}"), "description": null}))
            .collect::<Vec<_>>();
        assert_eq!(
            parse_labels(&serde_json::to_vec(&labels).expect("encode test labels"))
                .expect_err("too many labels")
                .code,
            "provider_response"
        );
    }
}
