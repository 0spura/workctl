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
    let providers = config::select_providers(
        cli::prescan_value(&argv, "--code-provider").as_deref(),
        cli::prescan_value(&argv, "--work-item-provider").as_deref(),
        cli::prescan_value(&argv, "--provider").as_deref(),
        &cwd,
    );

    let matches = match cli::command_tree(providers).try_get_matches_from(&argv) {
        Ok(matches) => matches,
        Err(error) => return report_parse_failure(providers, error),
    };
    let mut globals = match cli::GlobalArgs::from_arg_matches(&matches) {
        Ok(globals) => globals,
        Err(error) => return report_parse_failure(providers, error),
    };
    globals.code_provider = Some(providers.code);
    globals.work_item_provider = Some(providers.work_items);
    let command = match cli::ActiveCommand::from_matches(providers, &matches) {
        Ok(command) => command,
        Err(error) => return report_parse_failure(providers, error),
    };

    match commands::execute(globals, command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            output::json::write_error(&error);
            ExitCode::FAILURE
        }
    }
}

fn report_parse_failure(providers: config::ProviderSelection, error: clap::Error) -> ExitCode {
    match error.kind() {
        clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion => {
            let _ = error.print();
            ExitCode::SUCCESS
        }
        _ => {
            let message =
                wrong_group_hint(providers, &error).unwrap_or("invalid command-line arguments");
            output::json::write_error(&AppError::invalid_input(message));
            ExitCode::from(2)
        }
    }
}

fn wrong_group_hint(
    providers: config::ProviderSelection,
    error: &clap::Error,
) -> Option<&'static str> {
    let invalid = error.context().find_map(|(kind, value)| {
        (matches!(kind, clap::error::ContextKind::InvalidSubcommand)).then(|| value.to_string())
    })?;
    match (providers.code, invalid.as_str()) {
        (config::Provider::Gitlab, "pr") => {
            Some("GitLab merge requests use the `mr` command group")
        }
        (config::Provider::Github, "mr") => Some("GitHub pull requests use the `pr` command group"),
        _ => None,
    }
}
