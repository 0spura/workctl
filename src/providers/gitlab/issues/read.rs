use crate::domain::{AppError, Issue, IssueSummary};
use crate::providers::IssueQuery;
use crate::providers::gitlab::issues::{GitLabIssues, mapping};

pub(super) fn list(
    provider: &GitLabIssues,
    query: &IssueQuery,
) -> Result<Vec<IssueSummary>, AppError> {
    let mut args = vec![
        "issue".to_owned(),
        "list".to_owned(),
        "--output".to_owned(),
        "json".to_owned(),
        "--repo".to_owned(),
        provider.repo_argument(),
    ];
    match query.state.as_str() {
        "all" => args.push("--all".to_owned()),
        "closed" => args.push("--closed".to_owned()),
        _ => {}
    }
    for label in &query.labels {
        args.push("--label".to_owned());
        args.push(label.clone());
    }
    for (flag, value) in [
        ("--assignee", query.assignee.as_deref()),
        ("--author", query.author.as_deref()),
        ("--milestone", query.milestone.as_deref()),
        ("--search", query.search.as_deref()),
    ] {
        if let Some(value) = value {
            args.push(flag.to_owned());
            args.push(value.to_owned());
        }
    }
    args.push("--per-page".to_owned());
    args.push(query.limit.to_string());

    let mut issues = mapping::issue_summaries(&provider.run_glab(&args)?)?;
    issues.truncate(query.limit);
    Ok(issues)
}

pub(super) fn show(provider: &GitLabIssues, number: u64) -> Result<Issue, AppError> {
    let args = [
        "issue".to_owned(),
        "view".to_owned(),
        "--output".to_owned(),
        "json".to_owned(),
        "--repo".to_owned(),
        provider.repo_argument(),
        number.to_string(),
    ];
    mapping::issue(&provider.run_glab(&args)?)
}
