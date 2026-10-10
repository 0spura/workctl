mod github;
mod gitlab;
mod labels;
mod support;

use crate::cli::{ActiveCommand, GlobalArgs};
use crate::domain::AppError;

pub fn execute(mut globals: GlobalArgs, command: ActiveCommand) -> Result<(), AppError> {
    globals.resolved_format = support::resolve_output_format(globals.format)?;
    match command {
        ActiveCommand::Github(command) => github::execute(&globals, command),
        ActiveCommand::Gitlab(command) => gitlab::execute(&globals, command),
    }
}
