mod checkout;
mod checks;
mod comment;
mod create;
mod diff;
mod edit;
mod list;
mod lock;
mod merge;
mod ready;
mod revert;
mod review;
mod shared;
mod status;
mod update_branch;
mod view;

use crate::cli::GlobalArgs;
use crate::cli::github::prs::{PrAction, PrArgs};
use crate::domain::AppError;
use crate::output::{self, SuccessOutput};

pub(super) fn execute(globals: &GlobalArgs, args: PrArgs) -> Result<(), AppError> {
    let output = match args.action {
        PrAction::Create(args) => create::execute(globals, args)?,
        PrAction::Checkout(args) => {
            let number = args.number.0;
            checkout::execute(globals, args)?;
            SuccessOutput::Checkout { number }
        }
        PrAction::List(args) => list::execute(globals, &args)?,
        PrAction::View(args) => view::execute(globals, args)?,
        PrAction::Status(args) => status::execute(globals, args)?,
        PrAction::Diff(args) => diff::execute(globals, args)?,
        PrAction::Checks(args) => checks::execute(globals, args)?,
        PrAction::Review(args) => review::execute(globals, args)?,
        PrAction::Merge(args) => merge::execute(globals, args)?,
        PrAction::Edit(args) => edit::execute(globals, args)?,
        PrAction::Ready(args) => ready::execute(globals, args)?,
        PrAction::Comment(args) => comment::execute(globals, args)?,
        PrAction::Lock(args) => lock::execute(globals, args)?,
        PrAction::Revert(args) => revert::execute(globals, args)?,
        PrAction::UpdateBranch(args) => update_branch::execute(globals, args)?,
    };
    output::write(globals.resolved_format, &output)
}
