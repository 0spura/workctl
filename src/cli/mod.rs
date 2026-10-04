pub mod common;
pub mod github;
pub mod gitlab;

use std::ffi::OsString;

use clap::{ArgMatches, Args, Command, FromArgMatches, Subcommand};

use crate::config::Provider;

/// Flags accepted before or after any command, whatever the provider.
#[derive(Debug, Clone, Args)]
pub struct GlobalArgs {
    /// Work-item provider; defaults to the Git origin host
    #[arg(long, global = true, value_enum)]
    pub provider: Option<Provider>,
    /// Repository as OWNER/REPO or HOST/OWNER/REPO; GitLab also accepts GROUP/SUBGROUP/PROJECT
    #[arg(long, global = true, value_name = "REPO")]
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

/// The command parsed against the grammar of the resolved provider.
#[derive(Debug)]
pub enum ActiveCommand {
    Github(github::GithubCommand),
    Gitlab(gitlab::GitlabCommand),
}

impl ActiveCommand {
    pub fn from_matches(provider: Provider, matches: &ArgMatches) -> Result<Self, clap::Error> {
        Ok(match provider {
            Provider::Github => Self::Github(github::GithubCommand::from_arg_matches(matches)?),
            Provider::Gitlab => Self::Gitlab(gitlab::GitlabCommand::from_arg_matches(matches)?),
        })
    }
}

/// Builds the root command with only `provider`'s grammar attached.
///
/// Each provider owns its verbs and flags, so a command or flag that belongs to another
/// provider's CLI is a usage error instead of something the runtime has to reject.
pub fn command_tree(provider: Provider) -> Command {
    let root = GlobalArgs::augment_args(Command::new("workctl"));
    let root = match provider {
        Provider::Github => github::GithubCommand::augment_subcommands(root),
        Provider::Gitlab => gitlab::GitlabCommand::augment_subcommands(root),
    };
    // The provider grammar enum carries its own doc comment, so the neutral root identity is set
    // after the subcommands are attached.
    root.version(env!("CARGO_PKG_VERSION"))
        .about("Manage GitHub and GitLab work items from the terminal")
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
            found = tokens.next().and_then(|value| value.to_str()).map(str::to_owned);
        }
    }
    found
}
