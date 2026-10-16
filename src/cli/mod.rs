pub mod common;
pub mod github;
pub mod gitlab;
pub mod workflow;

use std::ffi::OsString;

use crate::config::{Provider, ProviderSelection};
use clap::{ArgMatches, Args, Command, FromArgMatches, Subcommand};

/// The selected providers are injected after offline worktree/config resolution.
#[derive(Debug, Clone, Args)]
pub struct GlobalArgs {
    #[arg(skip)]
    pub code_provider: Option<Provider>,
    #[arg(skip)]
    pub work_item_provider: Option<Provider>,
    /// Local Git worktree that supplies provider, repository, and project configuration
    #[arg(long, global = true, value_name = "PATH")]
    pub repo_path: Option<std::path::PathBuf>,
    /// Repository for the selected command's provider scope
    #[arg(long, short = 'R', global = true, value_name = "REPO")]
    pub repo: Option<String>,
    /// Output format; defaults to defaults.output.format, then compact text
    #[arg(long, global = true, value_enum, value_name = "FORMAT")]
    pub format: Option<OutputFormat>,
    /// Output format resolved before command dispatch; never a command-line argument
    #[arg(skip)]
    pub resolved_format: OutputFormat,
}

#[derive(Debug, Clone, Copy, Default, clap::ValueEnum)]
pub enum OutputFormat {
    Json,
    #[default]
    Text,
}

/// Parsed using the provider grammar that owns the selected command.
#[derive(Debug)]
pub enum ActiveCommand {
    Github(github::GithubCommand),
    Gitlab(gitlab::GitlabCommand),
    Workflow(workflow::WorkflowAction),
}

impl ActiveCommand {
    pub fn from_matches(
        providers: ProviderSelection,
        matches: &ArgMatches,
    ) -> Result<Self, clap::Error> {
        match (
            matches.subcommand_name(),
            providers.work_items,
            providers.code,
        ) {
            (Some("issue"), Some(Provider::Github), _) => Ok(Self::Github(
                github::GithubCommand::from_arg_matches(matches)?,
            )),
            (Some("issue"), Some(Provider::Gitlab), _) => Ok(Self::Gitlab(
                gitlab::GitlabCommand::from_arg_matches(matches)?,
            )),
            (Some("pr"), _, Some(Provider::Github)) => Ok(Self::Github(
                github::GithubCommand::from_arg_matches(matches)?,
            )),
            (Some("mr"), _, Some(Provider::Gitlab)) => Ok(Self::Gitlab(
                gitlab::GitlabCommand::from_arg_matches(matches)?,
            )),
            (Some("workflow"), _, _) => {
                let (_, workflow_matches) = matches.subcommand().expect("workflow subcommand");
                Ok(Self::Workflow(workflow::WorkflowAction::from_arg_matches(
                    workflow_matches,
                )?))
            }
            _ => Err(clap::Error::raw(
                clap::error::ErrorKind::InvalidSubcommand,
                "command does not belong to the selected provider",
            )),
        }
    }
}

/// Builds a mixed command tree: work-item commands follow the tracker, while PR/MR commands
/// follow the code host. A provider's static grammar is reused without unioning its flags with
/// another provider's grammar.
pub fn command_tree(providers: ProviderSelection) -> Command {
    let mut root = GlobalArgs::augment_args(Command::new("workctl"));
    let github_commands =
        github::GithubCommand::augment_subcommands(Command::new("provider-template"));
    let gitlab_commands =
        gitlab::GitlabCommand::augment_subcommands(Command::new("provider-template"));
    let issue = match providers.work_items.unwrap_or(Provider::Github) {
        Provider::Github => subcommand(&github_commands, "issue"),
        Provider::Gitlab => subcommand(&gitlab_commands, "issue"),
    };
    root = root.subcommand(issue);
    let code = match providers.code.unwrap_or(Provider::Github) {
        Provider::Github => subcommand(&github_commands, "pr"),
        Provider::Gitlab => subcommand(&gitlab_commands, "mr"),
    };
    root = root.subcommand(code);
    root = root.subcommand(workflow::WorkflowAction::augment_subcommands(Command::new(
        "workflow",
    )));
    root = root.subcommand(
        Command::new("update")
            .about("Update workctl to the latest commit with `cargo install`")
            .arg(
                clap::Arg::new("check")
                    .long("check")
                    .action(clap::ArgAction::SetTrue)
                    .help("Only report whether an update is available; do not install"),
            ),
    );
    root.version(env!("CARGO_PKG_VERSION"))
        .about("Manage work items and code-host requests")
        .after_help(
            "Provider resolution happens before command parsing:\n\
             1. `.workctl.json` and `.workctl.local.json` domain-specific settings select providers.\n\
             2. Otherwise, the selected target worktree's Git origin host selects the provider.\n\
             Run workctl from the target project's Git worktree, or use `--repo-path PATH` to select a local target worktree and its configuration.\n\
             `--repo` overrides only the remote repository scope within the selected provider.\n\
             `issue` follows the work-item provider; `pr`/`mr` follows the code-host provider.",
        )
}

fn subcommand(command: &Command, name: &str) -> Command {
    command
        .get_subcommands()
        .find(|subcommand| subcommand.get_name() == name)
        .expect("provider grammar declares its command")
        .clone()
}

/// Reads a global value from raw arguments before parsing, for error-format preservation.
///
/// `--format` can be read without a provider-specific command tree.
pub fn prescan_value(argv: &[OsString], name: &str) -> Option<String> {
    let prefix = format!("{name}=");
    let mut found = None;
    let mut tokens = argv.iter().skip(1);
    while let Some(token) = tokens.next() {
        let Some(token) = token.to_str() else {
            continue;
        };
        if token == "--" {
            break;
        }
        if let Some(value) = token.strip_prefix(&prefix) {
            found = Some(value.to_owned());
        } else if token == name {
            found = tokens
                .next()
                .and_then(|value| value.to_str())
                .map(str::to_owned);
        }
    }
    found
}
/// Reads a global local-path value before provider-specific grammar parsing.
pub fn prescan_path(argv: &[OsString], name: &str) -> Option<std::path::PathBuf> {
    let prefix = format!("{name}=");
    let mut tokens = argv.iter().skip(1);
    while let Some(token) = tokens.next() {
        if token == "--" {
            break;
        }
        if let Some(value) = token.as_encoded_bytes().strip_prefix(prefix.as_bytes()) {
            // SAFETY: the bytes come from this OsStr, split immediately after an ASCII `=`,
            // which is an encoded-character boundary on every supported platform.
            let value = unsafe { std::ffi::OsStr::from_encoded_bytes_unchecked(value) };
            return Some(std::path::PathBuf::from(value));
        }
        if token == name {
            return tokens.next().map(std::path::PathBuf::from);
        }
    }
    None
}
