pub mod common;
pub mod github;
pub mod gitlab;

use std::ffi::OsString;

use clap::{ArgMatches, Args, Command, FromArgMatches, Subcommand};

use crate::config::{Provider, ProviderSelection};

/// Provider flags remain command-global; each command resolves them in its owning domain.
#[derive(Debug, Clone, Args)]
pub struct GlobalArgs {
    /// Legacy provider selector; applies to code and work-item commands
    #[arg(long, global = true, value_enum)]
    pub provider: Option<Provider>,
    /// Code-host provider; defaults to codeProvider or the Git origin host
    #[arg(long = "code-provider", global = true, value_enum)]
    pub code_provider: Option<Provider>,
    /// Work-item provider; defaults to workItemProvider or the Git origin host
    #[arg(long = "work-item-provider", global = true, value_enum)]
    pub work_item_provider: Option<Provider>,
    /// Repository for the selected command's provider scope
    #[arg(long, short = 'R', global = true, value_name = "REPO")]
    pub repo: Option<String>,
    /// Success output format
    #[arg(long, global = true, value_enum, default_value = "json")]
    pub format: OutputFormat,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum OutputFormat {
    Json,
    Text,
}

/// Parsed using the provider grammar that owns the selected command.
#[derive(Debug)]
pub enum ActiveCommand {
    Github(github::GithubCommand),
    Gitlab(gitlab::GitlabCommand),
}

impl ActiveCommand {
    pub fn from_matches(
        providers: ProviderSelection,
        matches: &ArgMatches,
    ) -> Result<Self, clap::Error> {
        match matches.subcommand_name() {
            Some("issue") if providers.work_items == Provider::Github => Ok(Self::Github(
                github::GithubCommand::from_arg_matches(matches)?,
            )),
            Some("issue") if providers.work_items == Provider::Gitlab => Ok(Self::Gitlab(
                gitlab::GitlabCommand::from_arg_matches(matches)?,
            )),
            Some("pr") if providers.code == Provider::Github => Ok(Self::Github(
                github::GithubCommand::from_arg_matches(matches)?,
            )),
            Some("mr") if providers.code == Provider::Gitlab => Ok(Self::Gitlab(
                gitlab::GitlabCommand::from_arg_matches(matches)?,
            )),
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
    let issue = match providers.work_items {
        Provider::Github => subcommand(&github_commands, "issue"),
        Provider::Gitlab => subcommand(&gitlab_commands, "issue"),
    };
    root = root.subcommand(issue);
    let code = match providers.code {
        Provider::Github => subcommand(&github_commands, "pr"),
        Provider::Gitlab => subcommand(&gitlab_commands, "mr"),
    };
    root = root.subcommand(code);
    root.version(env!("CARGO_PKG_VERSION"))
        .about("Manage work items and code-host requests")
        .after_help(
            "Provider resolution happens before command parsing:\n\
             1. `--code-provider` and `--work-item-provider` override their respective domains.\n\
             2. Legacy `--provider` applies to both domains unless a domain-specific flag is supplied.\n\
             3. `.workctl.json` and `.workctl.local.json` domain-specific settings override legacy `provider`.\n\
             4. The Git origin host is the final fallback.\n\
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

/// Reads the last value of a global flag from the raw arguments, before parsing.
///
/// The grammar depends on the provider, so `--provider` has to be read without a command tree.
/// Argument values never start with `--`, and `--` ends the scan.
pub fn prescan_value(argv: &[OsString], name: &str) -> Option<String> {
    let prefix = format!("{name}=");
    let mut found = None;
    let mut tokens = argv.iter().skip(1);
    while let Some(token) = tokens.next() {
        let token = token.to_str()?;
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
