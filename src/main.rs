mod cli;
mod commands;
mod config;
mod decision_model;
mod domain;
mod events;
mod output;
mod process;
mod providers;

use std::ffi::OsString;
use std::path::Path;
use std::process::ExitCode;

use clap::{FromArgMatches, ValueEnum};
use domain::AppError;

fn main() -> ExitCode {
    let argv = std::env::args_os().collect::<Vec<OsString>>();
    let cwd = std::env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf());
    let repo_path = cli::prescan_path(&argv, "--repo-path");
    let target_root = match repo_path.as_deref() {
        Some(path) => match config::target_worktree_root(&cwd, path) {
            Ok(root) => root,
            Err(error) => {
                output::error::write(parse_error_format(&argv, None), &error);
                return ExitCode::FAILURE;
            }
        },
        None => cwd.clone(),
    };
    let providers = config::select_providers(&target_root);

    let matches = match cli::command_tree(providers).try_get_matches_from(&argv) {
        Ok(matches) => matches,
        Err(error) => return report_parse_failure(providers, &argv, &target_root, error),
    };
    let mut globals = match cli::GlobalArgs::from_arg_matches(&matches) {
        Ok(globals) => globals,
        Err(error) => return report_parse_failure(providers, &argv, &target_root, error),
    };
    globals.code_provider = providers.code;
    globals.work_item_provider = providers.work_items;
    if globals.repo_path.is_some() && std::env::set_current_dir(&target_root).is_err() {
        let error = AppError::context("could not enter the target project worktree");
        output::error::write(parse_error_format(&argv, Some(&target_root)), &error);
        return ExitCode::FAILURE;
    }
    let format = match commands::resolve_output_format(globals.format) {
        Ok(format) => format,
        Err(error) => {
            output::error::write(globals.format.unwrap_or_default(), &error);
            return ExitCode::FAILURE;
        }
    };
    globals.resolved_format = format;
    let missing_provider = match matches.subcommand_name() {
        Some("issue") => providers.work_items.is_none(),
        Some("pr" | "mr") => providers.code.is_none(),
        _ => false,
    };
    if missing_provider {
        let error = config::load_for_cwd(&target_root).err().unwrap_or_else(|| {
            AppError::context(
                "could not determine the command's provider from project configuration or Git origin",
            )
        });
        output::error::write(format, &error);
        return ExitCode::FAILURE;
    }
    let command = match cli::ActiveCommand::from_matches(providers, &matches) {
        Ok(command) => command,
        Err(error) => return report_parse_failure(providers, &argv, &target_root, error),
    };

    match commands::execute(globals, command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            output::error::write(format, &error);
            ExitCode::FAILURE
        }
    }
}

fn report_parse_failure(
    providers: config::ProviderSelection,
    argv: &[OsString],
    target_root: &Path,
    error: clap::Error,
) -> ExitCode {
    match error.kind() {
        clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion => {
            let _ = error.print();
            ExitCode::SUCCESS
        }
        _ => {
            let message =
                wrong_group_hint(providers, &error).unwrap_or("invalid command-line arguments");
            output::error::write(
                parse_error_format(argv, Some(target_root)),
                &AppError::invalid_input(message),
            );
            ExitCode::from(2)
        }
    }
}

fn parse_error_format(argv: &[OsString], target_root: Option<&Path>) -> cli::OutputFormat {
    match cli::prescan_value(argv, "--format") {
        Some(value) => cli::OutputFormat::from_str(&value, false).unwrap_or_default(),
        None => {
            let configured = target_root
                .and_then(|root| config::load_for_cwd(root).ok())
                .and_then(|config| config.defaults)
                .and_then(|defaults| defaults.output)
                .and_then(|output| output.format);
            match configured {
                Some(config::OutputFormatSetting::Json) => cli::OutputFormat::Json,
                Some(config::OutputFormatSetting::Text) | None => cli::OutputFormat::Text,
            }
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
        (Some(config::Provider::Gitlab), "pr") => {
            Some("GitLab merge requests use the `mr` command group")
        }
        (Some(config::Provider::Github), "mr") => {
            Some("GitHub pull requests use the `pr` command group")
        }
        _ => None,
    }
}
