use crate::cli::GlobalArgs;
use crate::cli::github::issues::{IssueAction, IssueArgs, ListArgs};
use crate::domain::{AppError, LabelSuggestion};
use crate::output::{self, SuccessOutput};
use crate::providers::github::issues::GitHubIssues;
use crate::providers::{IssuePatch, IssueQuery, NewIssue, WorkItemProvider, resolve_body_change};

use crate::commands::support;

const AUTO_LABEL_THRESHOLD: f64 = 0.8;

pub(super) fn execute(globals: &GlobalArgs, args: IssueArgs) -> Result<(), AppError> {
    let output = match args.action {
        IssueAction::Create(args) => {
            let body = support::optional_text(
                args.body.as_deref(),
                args.body_file.as_deref(),
                "use either --body or --body-file",
            )?
            .unwrap_or_default();
            let mut labels = args.labels;
            let automatic = labels.iter().any(|label| label == "@auto");
            labels.retain(|label| label != "@auto");
            let decision_model = automatic
                .then(crate::decision_model::DecisionModel::from_environment)
                .transpose()?;
            let provider = provider(globals)?;
            if let Some(model) = decision_model {
                let catalog = provider.labels()?;
                labels.extend(auto_label_names(&model, &args.title, &body, &catalog)?);
            }
            SuccessOutput::Issue(provider.create(&NewIssue {
                title: args.title,
                body,
                assignees: args.assignees,
                labels,
                milestone: args.milestone,
                projects: args.projects,
                attachments: args.attachments,
            })?)
        }
        IssueAction::List(args) => {
            let query = query(&args)?;
            let provider = provider(globals)?;
            SuccessOutput::Issues(provider.list(&query)?)
        }
        IssueAction::View { number } => {
            let provider = provider(globals)?;
            SuccessOutput::Issue(provider.show(number.0)?)
        }
        IssueAction::Edit(args) => {
            if args.clear_milestone && args.milestone.is_some() {
                return Err(AppError::invalid_input(
                    "--milestone and --clear-milestone cannot be used together",
                ));
            }
            if args.labels_remove.iter().any(|label| label == "@auto") {
                return Err(AppError::invalid_input(
                    "@auto is reserved for automatic label selection and cannot be removed",
                ));
            }
            let change = support::body_change(&args.change)?;
            let mut labels_add = args.labels_add;
            let automatic = labels_add.iter().any(|label| label == "@auto");
            labels_add.retain(|label| label != "@auto");
            if args.title.is_none()
                && change.is_none()
                && args.assignees_add.is_empty()
                && args.assignees_remove.is_empty()
                && labels_add.is_empty()
                && args.labels_remove.is_empty()
                && !automatic
                && args.milestone.is_none()
                && !args.clear_milestone
                && args.projects_add.is_empty()
                && args.projects_remove.is_empty()
                && args.attachments.is_empty()
            {
                return Err(AppError::invalid_input("edit requires a field to change"));
            }
            let decision_model = automatic
                .then(crate::decision_model::DecisionModel::from_environment)
                .transpose()?;
            let auto_only = automatic
                && args.title.is_none()
                && change.is_none()
                && args.assignees_add.is_empty()
                && args.assignees_remove.is_empty()
                && labels_add.is_empty()
                && args.labels_remove.is_empty()
                && args.milestone.is_none()
                && !args.clear_milestone
                && args.projects_add.is_empty()
                && args.projects_remove.is_empty()
                && args.attachments.is_empty();
            let provider = provider(globals)?;
            let mut expect_updated_at = args.expect_updated_at;
            if let Some(model) = decision_model {
                let current = provider.show(args.number.0)?;
                if expect_updated_at
                    .as_deref()
                    .is_some_and(|expected| expected != current.updated_at)
                {
                    return Err(AppError::conflict());
                }
                let title = args.title.as_deref().unwrap_or(&current.title);
                let body = match change.as_ref() {
                    None => current.body.clone(),
                    Some(change) => resolve_body_change(&current.body, change)?,
                };
                let catalog = provider.labels()?;
                labels_add.extend(auto_label_names(&model, title, &body, &catalog)?);
                if labels_add.is_empty() && auto_only {
                    return output::write(globals.format, &SuccessOutput::Issue(current));
                }
                expect_updated_at.get_or_insert(current.updated_at);
            }
            SuccessOutput::Issue(provider.edit(
                args.number.0,
                &IssuePatch {
                    title: args.title,
                    body: change,
                    assignees_add: args.assignees_add,
                    assignees_remove: args.assignees_remove,
                    labels_add,
                    labels_remove: args.labels_remove,
                    milestone: args.milestone,
                    clear_milestone: args.clear_milestone,
                    projects_add: args.projects_add,
                    projects_remove: args.projects_remove,
                    attachments: args.attachments,
                    expect_updated_at,
                },
            )?)
        }
    };
    output::write(globals.format, &output)
}
fn auto_label_names(
    model: &crate::decision_model::DecisionModel,
    title: &str,
    description: &str,
    catalog: &[crate::domain::RepositoryLabel],
) -> Result<Vec<String>, AppError> {
    Ok(auto_label_names_from_suggestions(model.suggest(
        crate::decision_model::DecisionInput {
            title,
            description,
            labels: catalog,
        },
    )?))
}

fn auto_label_names_from_suggestions(suggestions: Vec<LabelSuggestion>) -> Vec<String> {
    suggestions
        .into_iter()
        .filter(|suggestion| suggestion.probability >= AUTO_LABEL_THRESHOLD)
        .map(|suggestion| suggestion.label)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::auto_label_names_from_suggestions;
    use crate::domain::LabelSuggestion;

    #[test]
    fn automatic_labels_include_only_scores_at_or_above_threshold() {
        let labels = auto_label_names_from_suggestions(vec![
            LabelSuggestion {
                label: "below".to_owned(),
                probability: 0.79,
            },
            LabelSuggestion {
                label: "boundary".to_owned(),
                probability: 0.8,
            },
            LabelSuggestion {
                label: "above".to_owned(),
                probability: 0.91,
            },
        ]);
        assert_eq!(labels, ["boundary", "above"]);
    }
}

fn query(args: &ListArgs) -> Result<IssueQuery, AppError> {
    Ok(IssueQuery {
        state: args.state.as_str().to_owned(),
        limit: args.limit,
        labels: args.labels.clone(),
        assignee: args.assignee.clone(),
        author: args.author.clone(),
        mention: args.mention.clone(),
        milestone: args.milestone.clone(),
        search: args.search.clone(),
        issue_type: args.issue_type.clone(),
    })
}

fn provider(globals: &GlobalArgs) -> Result<GitHubIssues, AppError> {
    let provider = GitHubIssues::new(support::resolve_repo(globals.provider, globals.repo.as_deref())?);
    provider.authenticate()?;
    Ok(provider)
}

