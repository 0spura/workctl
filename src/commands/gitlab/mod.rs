mod issues;

use crate::cli::GlobalArgs;
use crate::cli::gitlab::GitlabCommand;
use crate::domain::AppError;

pub(super) fn execute(globals: &GlobalArgs, command: GitlabCommand) -> Result<(), AppError> {
    match command {
        GitlabCommand::Issue(args) => issues::execute(globals, args),
    }
}
