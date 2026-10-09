use crate::domain::RepositoryLabel;
use crate::domain::{AppError, Issue, IssueSummary};
use crate::providers::gitlab::issues::{GitLabIssueQuery, GitLabIssues, mapping};

pub(super) fn list(
    provider: &GitLabIssues,
    query: &GitLabIssueQuery,
) -> Result<Vec<IssueSummary>, AppError> {
    let mut args = vec![
        "issue".to_owned(),
        "list".to_owned(),
        "--output".to_owned(),
        "json".to_owned(),
        "--repo".to_owned(),
        provider.repo_argument(),
    ];
    if query.all {
        args.push("--all".to_owned());
    }
    if query.closed {
        args.push("--closed".to_owned());
    }
    if query.confidential {
        args.push("--confidential".to_owned());
    }
    for (flag, values) in [
        ("--label", &query.labels),
        ("--not-label", &query.not_labels),
        ("--not-assignee", &query.not_assignees),
        ("--not-author", &query.not_authors),
    ] {
        for value in values {
            args.extend([flag.to_owned(), value.clone()]);
        }
    }
    for (flag, value) in [
        ("--assignee", query.assignee.as_deref()),
        ("--author", query.author.as_deref()),
        ("--milestone", query.milestone.as_deref()),
        ("--search", query.search.as_deref()),
        ("--in", query.in_fields.as_deref()),
        ("--issue-type", query.issue_type.as_deref()),
        ("--iteration", query.iteration.as_deref()),
        ("--order", query.order.as_deref()),
        ("--sort", query.sort.as_deref()),
    ] {
        if let Some(value) = value {
            args.extend([flag.to_owned(), value.to_owned()]);
        }
    }
    args.extend(["--page".to_owned(), query.page.to_string()]);
    args.extend(["--per-page".to_owned(), query.per_page.to_string()]);

    let mut issues = mapping::issue_summaries(&provider.run_glab(&args)?)?;
    issues.truncate(query.per_page);
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

pub(super) fn labels(provider: &GitLabIssues) -> Result<Vec<RepositoryLabel>, AppError> {
    const PAGE_SIZE: usize = 100;
    const MAX_LABELS: usize = 1000;
    let mut labels = Vec::new();
    let mut names = std::collections::HashSet::new();
    for page in 1..=MAX_LABELS / PAGE_SIZE + 1 {
        let args = [
            "label".to_owned(),
            "list".to_owned(),
            "--output".to_owned(),
            "json".to_owned(),
            "--repo".to_owned(),
            provider.repo_argument(),
            "--per-page".to_owned(),
            PAGE_SIZE.to_string(),
            "--page".to_owned(),
            page.to_string(),
        ];
        let page_labels = mapping::labels(&provider.run_glab(&args)?)?;
        if page_labels.is_empty() {
            return Ok(labels);
        }
        if labels.len() + page_labels.len() > MAX_LABELS {
            return Err(AppError::decision_input_limit());
        }
        if page_labels
            .iter()
            .any(|label| !names.insert(label.name.clone()))
        {
            return Err(AppError::provider_response());
        }
        labels.extend(page_labels);
    }
    Ok(labels)
}
