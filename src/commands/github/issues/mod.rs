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
    let edit_repository = match &args.action {
        IssueAction::Edit(args) => Some(edit::resolve_repository(globals, args)?),
        _ => None,
    };
    let records_events = matches!(
        &args.action,
        IssueAction::Create(_)
            | IssueAction::Edit(_)
            | IssueAction::Comment(_)
            | IssueAction::Lock(_)
    ) || matches!(&args.action, IssueAction::Develop(args) if !args.list);
    let event_endpoints = if records_events {
        let cwd = std::env::current_dir()
            .map_err(|_| AppError::context("could not determine current directory"))?;
        crate::events::validate_for_cwd(
            &cwd,
            "github",
            edit_repository.as_deref().or(globals.repo.as_deref()),
        )?
    } else {
        crate::events::disabled()
    };
    let output = match args.action {
        IssueAction::Create(args) => create::execute(globals, args),
        IssueAction::List(args) => list::execute(globals, &args),
        IssueAction::Blockers(args) => blockers::execute(globals, args),
        IssueAction::View(args) => view::execute(globals, args),
        IssueAction::Edit(args) => edit::execute(
            args,
            edit_repository.expect("edit repository was validated before event preflight"),
        ),
        IssueAction::Comment(args) => comment::execute(globals, args),
        IssueAction::Lock(args) => lock::execute(globals, args),
        IssueAction::Develop(args) => develop::execute(globals, args),
    };
    let output = match output {
        Ok(output) => output,
        Err(error) => {
            crate::events::record_partial(globals, "github", &error, &event_endpoints);
            return Err(error);
        }
    };
    crate::events::record_success(globals, "github", &output, &event_endpoints);
    output::write(globals.resolved_format, &output)
}
