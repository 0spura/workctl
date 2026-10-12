use crate::cli::{OutputFormat, workflow::WorkflowAction};
use crate::config;
use crate::domain::AppError;
use crate::output::{self, SuccessOutput};

pub(crate) fn execute(format: OutputFormat, action: WorkflowAction) -> Result<(), AppError> {
    let cwd = std::env::current_dir()
        .map_err(|_| AppError::context("could not determine current directory"))?;
    let root = config::repository_root(&cwd)?;
    let scope = std::fs::canonicalize(&root)
        .map_err(|_| AppError::context("could not determine repository scope"))?
        .to_string_lossy()
        .into_owned();
    let current_config = config::load_for_cwd(&root)?;
    let approved = config::workflow_approval_status(&current_config, &root)?;
    let fingerprint = if approved {
        current_config
            .workflow_approval
            .as_deref()
            .unwrap_or_default()
    } else {
        ""
    };
    let output = match action {
        WorkflowAction::Approve => {
            let fingerprint = config::approve_workflow(&root)?;
            SuccessOutput::WorkflowStatus {
                status: "approved".to_owned(),
                actions: crate::events::workflow_action_counts(&scope, &fingerprint)?,
            }
        }
        WorkflowAction::Status => {
            let status = if current_config.workflow.is_none() {
                "no rules"
            } else if approved {
                "approved"
            } else {
                "approval required"
            };
            SuccessOutput::WorkflowStatus {
                status: status.to_owned(),
                actions: crate::events::workflow_action_counts(&scope, fingerprint)?,
            }
        }
        WorkflowAction::Actions { limit } => {
            require_approval(approved)?;
            SuccessOutput::WorkflowActions(crate::events::list_workflow_actions(
                &scope,
                fingerprint,
                limit,
            )?)
        }
        WorkflowAction::Resolve {
            id,
            applied,
            retry: _,
        } => {
            require_approval(approved)?;
            let resolution = if applied {
                crate::events::ActionResolution::Applied
            } else {
                crate::events::ActionResolution::Retry
            };
            let state =
                crate::events::resolve_workflow_action(&id, resolution, &scope, fingerprint)?;
            SuccessOutput::WorkflowResolution { id, state }
        }
    };
    output::write(format, &output)
}

fn require_approval(approved: bool) -> Result<(), AppError> {
    if approved {
        Ok(())
    } else {
        Err(AppError::config("workflow approval is required"))
    }
}
