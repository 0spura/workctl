use crate::cli::GlobalArgs;
use crate::cli::gitlab::issues::CreateArgs;
use crate::commands::labels::automatic_label_names;
use crate::commands::support;
use crate::decision_model::{DecisionInput, DecisionModel};
use crate::domain::{AppError, DecisionCandidate};
use crate::output::SuccessOutput;
use crate::providers::gitlab::issues::{GitLabIssueCreate, GitLabIssues};

pub(super) fn execute(globals: &GlobalArgs, args: CreateArgs) -> Result<SuccessOutput, AppError> {
    let description = support::optional_text(
        args.description.as_deref(),
        args.description_file.as_deref(),
        "use either --description or --description-file",
    )?
    .ok_or(AppError::invalid_input(
        "issue create requires --description or --description-file",
    ))?;
    validate_create_assignees(&args.assignees)?;

    let mut labels = args.labels;
    let automatic = labels.iter().any(|label| label == "@auto");
    labels.retain(|label| label != "@auto");
    let model = automatic
        .then(DecisionModel::from_environment)
        .transpose()?;
    let repo = support::resolve_repo(globals.work_item_provider, globals.repo.as_deref())?;
    let provider = GitLabIssues::new(repo)?;
    provider.authenticate()?;
    if let Some(model) = model {
        let candidates = provider
            .labels()?
            .into_iter()
            .map(|label| DecisionCandidate {
                name: label.name,
                description: label.description,
            })
            .collect::<Vec<_>>();
        let scores = model.suggest(DecisionInput {
            title: &args.title,
            description: &description,
            candidates: &candidates,
        })?;
        for label in automatic_label_names(&scores, &candidates) {
            if !labels.contains(&label) {
                labels.push(label);
            }
        }
    }

    Ok(SuccessOutput::Issue(provider.create(
        &GitLabIssueCreate {
            title: args.title,
            description,
            labels,
            assignees: args.assignees,
            milestone: args.milestone,
            confidential: args.confidential,
            weight: args.weight,
            due_date: args.due_date,
            epic: args.epic,
            linked_issues: args.linked_issues,
            link_type: args.link_type,
            linked_mr: args.linked_mr,
            time_estimate: args.time_estimate,
            time_spent: args.time_spent,
            template: args.template,
        },
    )?))
}

fn validate_create_assignees(assignees: &[String]) -> Result<(), AppError> {
    if assignees
        .iter()
        .flat_map(|value| value.split(','))
        .any(|username| username.is_empty() || matches!(username.as_bytes()[0], b'+' | b'-' | b'!'))
    {
        return Err(AppError::invalid_input(
            "issue create assignees must be plain usernames without +/-/! prefixes",
        ));
    }
    Ok(())
}
