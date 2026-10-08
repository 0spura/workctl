use crate::cli::GlobalArgs;
use crate::domain::{AppError, DecisionCandidate, DecisionScore};
use crate::providers::WorkItemProvider;
use crate::providers::github::issues::GitHubIssues;

use crate::commands::support;

pub(super) const AUTO_LABEL_THRESHOLD: f64 = 0.8;

pub(super) fn filter_label_catalog(
    catalog: Vec<crate::domain::RepositoryLabel>,
    candidates: &[String],
) -> Result<Vec<crate::domain::RepositoryLabel>, AppError> {
    if candidates.is_empty() {
        return Ok(catalog);
    }
    let mut selected = Vec::with_capacity(candidates.len());
    for name in candidates {
        let label = catalog
            .iter()
            .find(|label| label.name == *name)
            .ok_or(AppError::config(
                "a configured label candidate does not exist in the repository",
            ))?;
        selected.push(label.clone());
    }
    Ok(selected)
}

pub(super) fn automatic_label_names(
    scores: &[DecisionScore],
    labels: &[DecisionCandidate],
) -> Vec<String> {
    scores
        .iter()
        .filter(|score| score.probability >= AUTO_LABEL_THRESHOLD)
        .filter(|score| labels.iter().any(|label| label.name == score.candidate))
        .map(|score| score.candidate.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::automatic_label_names;
    use crate::domain::{DecisionCandidate, DecisionScore};

    #[test]
    fn automatic_labels_include_only_scores_at_or_above_threshold() {
        let labels = [
            DecisionCandidate {
                name: "below".to_owned(),
                description: None,
            },
            DecisionCandidate {
                name: "boundary".to_owned(),
                description: None,
            },
            DecisionCandidate {
                name: "above".to_owned(),
                description: None,
            },
        ];
        let scores = vec![
            DecisionScore {
                candidate: "below".to_owned(),
                probability: 0.79,
            },
            DecisionScore {
                candidate: "boundary".to_owned(),
                probability: 0.8,
            },
            DecisionScore {
                candidate: "above".to_owned(),
                probability: 0.91,
            },
            // A Project field option, not a repository label: never returned as a label.
            DecisionScore {
                candidate: "Priority=High".to_owned(),
                probability: 0.99,
            },
        ];
        assert_eq!(
            automatic_label_names(&scores, &labels),
            ["boundary", "above"]
        );
    }
}

pub(super) fn provider(globals: &GlobalArgs) -> Result<GitHubIssues, AppError> {
    let provider = GitHubIssues::new(support::resolve_repo(
        globals.provider,
        globals.repo.as_deref(),
    )?);
    provider.authenticate()?;
    Ok(provider)
}
