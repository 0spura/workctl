mod create;
mod develop;
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
    let cwd = std::env::current_dir()
        .map_err(|_| AppError::context("could not determine current directory"))?;
    let event_endpoints = crate::events::validate_for_cwd(&cwd, "gitlab", globals.repo.as_deref())?;
    let repository = event_endpoints.repository.clone().unwrap_or_default();
    let issue_id = match &args.action {
        IssueAction::View(args) => Some(args.number.0),
        IssueAction::Update(args) => Some(args.number.0),
        IssueAction::Close(args)
        | IssueAction::Reopen(args)
        | IssueAction::Subscribe(args)
        | IssueAction::Unsubscribe(args) => Some(args.number.0),
        IssueAction::Develop(args) => Some(args.number.0),
        IssueAction::Note(args) => Some(args.number.0),
        _ => None,
    };
    let tracking =
        crate::events::work_tracking::Invocation::begin(&cwd, "gitlab", &repository, issue_id);
    let output = match args.action {
        IssueAction::Create(args) => create::execute(globals, args),
        IssueAction::List(args) => list::execute(globals, &args),
        IssueAction::View(args) => view::execute(globals, args),
        IssueAction::Develop(args) => develop::execute(globals, args),
        IssueAction::Update(args) => update::execute(globals, args),
        IssueAction::Close(args) => state::execute(globals, args, state::Operation::Close),
        IssueAction::Reopen(args) => state::execute(globals, args, state::Operation::Reopen),
        IssueAction::Subscribe(args) => state::execute(globals, args, state::Operation::Subscribe),
        IssueAction::Unsubscribe(args) => {
            state::execute(globals, args, state::Operation::Unsubscribe)
        }
        IssueAction::Note(args) => note::execute(globals, args),
    };
    let output = match output {
        Ok(output) => output,
        Err(error) => {
            tracking.finish(None);
            crate::events::record_partial(globals, "gitlab", &error, &event_endpoints);
            return Err(error);
        }
    };
    tracking.finish(Some(&output));
    crate::events::record_success(globals, "gitlab", &output, &event_endpoints);
    output::write(globals.resolved_format, &output)
}
