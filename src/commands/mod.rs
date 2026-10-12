mod github;
mod gitlab;
pub(crate) mod labels;
pub(crate) mod support;
mod workflow;

pub(crate) use support::resolve_output_format;

use crate::cli::{ActiveCommand, GlobalArgs};
use crate::domain::AppError;

pub fn execute(globals: GlobalArgs, command: ActiveCommand) -> Result<(), AppError> {
    match command {
        ActiveCommand::Github(command) => github::execute(&globals, command),
        ActiveCommand::Gitlab(command) => gitlab::execute(&globals, command),
        ActiveCommand::Workflow(action) => workflow::execute(globals.resolved_format, action),
    }
}
