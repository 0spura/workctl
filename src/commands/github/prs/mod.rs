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
    let records_events = matches!(
        &args.action,
        PrAction::Create(_)
            | PrAction::Review(_)
            | PrAction::Merge(_)
            | PrAction::Edit(_)
            | PrAction::Ready(_)
            | PrAction::Comment(_)
            | PrAction::Lock(_)
            | PrAction::Revert(_)
            | PrAction::UpdateBranch(_)
    );
    let mut event_endpoints = if records_events {
        let cwd = std::env::current_dir()
            .map_err(|_| AppError::context("could not determine current directory"))?;
        crate::events::validate_for_cwd(&cwd, "github", globals.repo.as_deref())?
    } else {
        crate::events::disabled()
    };
    if event_endpoints.workflow.is_some() {
        let repository = crate::commands::support::resolve_repo(
            Some(crate::config::Provider::Github),
            globals.repo.as_deref(),
        )?;
        match &args.action {
            PrAction::Create(args) => crate::events::validate_pr_reference_delta(
                &mut event_endpoints,
                &repository,
                None,
                &args.closes,
                &[],
            )?,
            PrAction::Edit(args) => crate::events::validate_pr_reference_delta(
                &mut event_endpoints,
                &repository,
                Some(args.number.0),
                &args.closes,
                &args.remove_closes,
            )?,
            _ => {}
        }
    }
    let output = match args.action {
        PrAction::Create(args) => create::execute(globals, args),
        PrAction::Checkout(args) => {
            let number = args.number.0;
            checkout::execute(globals, args).map(|()| SuccessOutput::Checkout { number })
        }
        PrAction::List(args) => list::execute(globals, &args),
        PrAction::View(args) => view::execute(globals, args),
        PrAction::Status(args) => status::execute(globals, args),
        PrAction::Diff(args) => diff::execute(globals, args),
        PrAction::Checks(args) => checks::execute(globals, args),
        PrAction::Review(args) => review::execute(globals, args),
        PrAction::Merge(args) => merge::execute(globals, args),
        PrAction::Edit(args) => edit::execute(globals, args),
        PrAction::Ready(args) => ready::execute(globals, args),
        PrAction::Comment(args) => comment::execute(globals, args),
        PrAction::Lock(args) => lock::execute(globals, args),
        PrAction::Revert(args) => revert::execute(globals, args),
        PrAction::UpdateBranch(args) => update_branch::execute(globals, args),
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
