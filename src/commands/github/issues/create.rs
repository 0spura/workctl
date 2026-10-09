use crate::cli::GlobalArgs;
use crate::cli::github::issues::CreateArgs;
use crate::domain::{AppError, DecisionCandidate};
use crate::output::SuccessOutput;
use crate::providers::github::issues::GitHubIssues;
use crate::providers::{NewIssue, WorkItemProvider};

use crate::commands::support;

use super::shared::filter_label_catalog;
use crate::commands::labels::{AUTO_LABEL_THRESHOLD, automatic_label_names};

pub(super) fn execute(globals: &GlobalArgs, args: CreateArgs) -> Result<SuccessOutput, AppError> {
    let body = support::optional_text(
        args.body.as_deref(),
        args.body_file.as_deref(),
        "use either --body or --body-file",
    )?
    .unwrap_or_default();
    let mut explicit_labels = args.labels;
    let automatic = explicit_labels.iter().any(|label| label == "@auto");
    explicit_labels.retain(|label| label != "@auto");
    let repo = support::resolve_repo(globals.work_item_provider, globals.repo.as_deref())?;
    let defaults = support::github_issue_defaults(&repo)?;
    let mut assignees = args.assignees;
    if assignees.is_empty() {
        assignees = defaults
            .as_ref()
            .map(|value| value.assignees.clone())
            .unwrap_or_default();
    }
    let mut labels = defaults
        .as_ref()
        .map(|value| value.labels.clone())
        .unwrap_or_default();
    for label in explicit_labels {
        if !labels.contains(&label) {
            labels.push(label);
        }
    }
    let cli_project_fields = args.project_fields;
    let mut project_fields = defaults
        .as_ref()
        .and_then(|value| value.project.as_ref())
        .map(|value| value.fields.clone())
        .unwrap_or_default();
    for assignment in cli_project_fields {
        let (name, value) = assignment
            .split_once('=')
            .filter(|(name, value)| !name.trim().is_empty() && !value.trim().is_empty())
            .ok_or(AppError::invalid_input(
                "--project-field must be NAME=VALUE",
            ))?;
        project_fields.insert(name.trim().to_owned(), value.trim().to_owned());
    }
    let project_profile = defaults
        .as_ref()
        .and_then(|value| value.project.clone())
        .filter(|profile| {
            args.projects.is_empty()
                || !project_fields.is_empty()
                || !profile.auto_select_fields.is_empty()
        });
    if (!project_fields.is_empty()
        || project_profile
            .as_ref()
            .is_some_and(|profile| !profile.auto_select_fields.is_empty()))
        && args.projects.len() > 1
    {
        return Err(AppError::invalid_input(
            "Project field assignment requires exactly one effective Project",
        ));
    }
    let auto_project_fields = project_profile.as_ref().is_some_and(|profile| {
        profile
            .auto_select_fields
            .iter()
            .any(|name| !project_fields.contains_key(name))
    });
    // Model configuration is resolved before any remote call: `@auto` and at least one
    // unclaimed Project field both require a model before the issue exists.
    let decision_model = (automatic || auto_project_fields)
        .then(crate::decision_model::DecisionModel::from_environment)
        .transpose()?;
    if !project_fields.is_empty() && project_profile.is_none() {
        return Err(AppError::invalid_input(
            "--project-field requires an in-scope configured GitHub Project",
        ));
    }
    let project_title = (args.projects.len() == 1).then(|| args.projects[0].clone());
    let provider = GitHubIssues::new(repo);
    provider.authenticate()?;
    let mut project_plan = project_profile
        .map(|profile| {
            provider.plan_project(
                &profile,
                project_fields.into_iter().collect(),
                project_title,
            )
        })
        .transpose()?;
    // One decision call covers the repository labels and the auto-selected Project fields.
    let mut candidates = Vec::new();
    let mut label_candidates = 0;
    if automatic {
        let catalog = filter_label_catalog(
            provider.labels()?,
            defaults
                .as_ref()
                .map(|value| value.label_candidates.as_slice())
                .unwrap_or(&[]),
        )?;
        label_candidates = catalog.len();
        candidates.extend(catalog.into_iter().map(|label| DecisionCandidate {
            name: label.name,
            description: label.description,
        }));
    }
    if let Some(plan) = project_plan.as_ref() {
        let options = plan.candidates();
        if candidates
            .iter()
            .any(|label| options.iter().any(|option| option.name == label.name))
        {
            return Err(AppError::config(
                "an automatically selected GitHub Project field collides with a repository label",
            ));
        }
        candidates.extend(options);
    }
    if let Some(model) = decision_model {
        let scores = model.suggest(crate::decision_model::DecisionInput {
            title: &args.title,
            description: &body,
            candidates: &candidates,
        })?;
        for label in automatic_label_names(&scores, &candidates[..label_candidates]) {
            if !labels.contains(&label) {
                labels.push(label);
            }
        }
        if let Some(plan) = project_plan.as_mut() {
            plan.choose_from_scores(&scores, AUTO_LABEL_THRESHOLD)?;
        }
    }
    let new_issue = NewIssue {
        title: args.title,
        body,
        assignees,
        labels,
        milestone: args.milestone,
        projects: args.projects,
        attachments: args.attachments,
    };
    let created = match project_plan.as_ref() {
        Some(plan) => provider.create_with_project(&new_issue, plan)?,
        None => provider.create(&new_issue)?,
    };
    Ok(SuccessOutput::Issue(created))
}
