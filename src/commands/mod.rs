mod github;
mod gitlab;
mod support;

use crate::cli::{ActiveCommand, GlobalArgs};
use crate::domain::AppError;

pub fn execute(globals: GlobalArgs, command: ActiveCommand) -> Result<(), AppError> {
    match command {
        ActiveCommand::Github(command) => github::execute(&globals, command),
        ActiveCommand::Gitlab(command) => gitlab::execute(&globals, command),
    }
}
