mod cli;
mod commands;
mod config;
mod decision_model;
mod domain;
mod output;
mod process;
mod providers;

use std::ffi::OsString;
use std::path::Path;
use std::process::ExitCode;

use clap::FromArgMatches;
use domain::AppError;

fn main() -> ExitCode {
    let argv = std::env::args_os().collect::<Vec<OsString>>();
    let cwd = std::env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf());
    let provider = config::select_provider(cli::prescan_value(&argv, "--provider").as_deref(), &cwd);

    let matches = match cli::command_tree(provider).try_get_matches_from(&argv) {
        Ok(matches) => matches,
        Err(error) => return report_parse_failure(provider, error),
    };
    let globals = match cli::GlobalArgs::from_arg_matches(&matches) {
        Ok(globals) => globals,
        Err(error) => return report_parse_failure(provider, error),
    };
    let command = match cli::ActiveCommand::from_matches(provider, &matches) {
        Ok(command) => command,
        Err(error) => return report_parse_failure(provider, error),
    };

    match commands::execute(globals, command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            output::json::write_error(&error);
            ExitCode::FAILURE
        }
    }
}

fn report_parse_failure(provider: config::Provider, error: clap::Error) -> ExitCode {
    match error.kind() {
        clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion => {
            let _ = error.print();
            ExitCode::SUCCESS
        }
        _ => {
            let message = wrong_group_hint(provider, &error)
                .unwrap_or("invalid command-line arguments");
            output::json::write_error(&AppError::invalid_input(message));
            ExitCode::from(2)
        }
    }
}

/// Names the group a caller probably wanted when they use the other provider's name for it.
///
/// `pr` is the GitHub group; GitLab spells merge requests `mr`. The hint says what GitLab calls
/// them and that the group does not exist yet, so it never implies a command that is not there.
fn wrong_group_hint(provider: config::Provider, error: &clap::Error) -> Option<&'static str> {
    if provider != config::Provider::Gitlab {
        return None;
    }
    let named_pr = error.context().any(|(kind, value)| {
        matches!(kind, clap::error::ContextKind::InvalidSubcommand) && value.to_string() == "pr"
    });
    named_pr.then_some(
        "the GitLab grammar has no `pr` group; GitLab merge requests use `mr`, which is not implemented yet",
    )
}
