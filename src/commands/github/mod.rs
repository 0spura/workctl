mod issues;
mod prs;

use crate::cli::GlobalArgs;
use crate::cli::github::GithubCommand;
use crate::domain::AppError;

pub(super) fn execute(globals: &GlobalArgs, command: GithubCommand) -> Result<(), AppError> {
    match command {
        GithubCommand::Issue(args) => issues::execute(globals, args),
        GithubCommand::Pr(args) => prs::execute(globals, args),
    }
}
