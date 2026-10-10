mod create;
mod list;
mod note;
mod shared;
mod state;
mod update;
mod view;

use crate::cli::GlobalArgs;
use crate::cli::gitlab::issues::{IssueAction, IssueArgs};
use crate::domain::AppError;
use crate::output;

pub(super) fn execute(globals: &GlobalArgs, args: IssueArgs) -> Result<(), AppError> {
    let output = match args.action {
        IssueAction::Create(args) => create::execute(globals, args)?,
        IssueAction::List(args) => list::execute(globals, &args)?,
        IssueAction::View(args) => view::execute(globals, args)?,
        IssueAction::Update(args) => update::execute(globals, args)?,
        IssueAction::Close(args) => state::execute(globals, args, state::Operation::Close)?,
        IssueAction::Reopen(args) => state::execute(globals, args, state::Operation::Reopen)?,
        IssueAction::Subscribe(args) => state::execute(globals, args, state::Operation::Subscribe)?,
        IssueAction::Unsubscribe(args) => {
            state::execute(globals, args, state::Operation::Unsubscribe)?
        }
        IssueAction::Note(args) => note::execute(globals, args)?,
    };
    output::write(globals.resolved_format, &output)
}
