mod blockers;
mod comment;
mod create;
mod develop;
mod edit;
mod list;
mod lock;
mod shared;
mod view;

use crate::cli::GlobalArgs;
use crate::cli::github::issues::{IssueAction, IssueArgs};
use crate::domain::AppError;
use crate::output;

pub(super) fn execute(globals: &GlobalArgs, args: IssueArgs) -> Result<(), AppError> {
    let output = match args.action {
        IssueAction::Create(args) => create::execute(globals, args)?,
        IssueAction::List(args) => list::execute(globals, &args)?,
        IssueAction::Blockers(args) => blockers::execute(globals, args)?,
        IssueAction::View(args) => view::execute(globals, args)?,
        IssueAction::Edit(args) => edit::execute(globals, args)?,
        IssueAction::Comment(args) => comment::execute(globals, args)?,
        IssueAction::Lock(args) => lock::execute(globals, args)?,
        IssueAction::Develop(args) => develop::execute(globals, args)?,
    };
    output::write(globals.format, &output)
}
