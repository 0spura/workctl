use std::fs;
use std::io::Read;
use std::path::Path;

use crate::cli::common::BodyChangeArgs;
use crate::config::{self, Provider};
use crate::domain::AppError;
use crate::providers::BodyChange;

/// Upper bound for any body text read from a file or standard input.
const MAX_TEXT_BYTES: u64 = 1024 * 1024;

/// Resolves a value given either inline or as a file, rejecting both at once.
pub(super) fn optional_text(
    inline: Option<&str>,
    file: Option<&str>,
    conflict: &'static str,
) -> Result<Option<String>, AppError> {
    match (inline, file) {
        (Some(_), Some(_)) => Err(AppError::invalid_input(conflict)),
        (Some(text), None) => Ok(Some(text.to_owned())),
        (None, Some(file)) => Ok(Some(read_source(file)?)),
        (None, None) => Ok(None),
    }
}

pub(super) fn body_change(args: &BodyChangeArgs) -> Result<Option<BodyChange>, AppError> {
    let replacements = [
        args.body.is_some(),
        args.body_file.is_some(),
        args.append_body.is_some(),
        args.append_body_file.is_some(),
        args.replace_section.is_some(),
        args.patch_file.is_some(),
    ];
    if replacements.iter().filter(|present| **present).count() > 1 {
        return Err(AppError::invalid_input(
            "use only one of --body, --body-file, --append-body, --append-body-file, --replace-section, or --patch-file",
        ));
    }
    if args.replace_section.is_some() {
        let text = match (
            args.section_body.as_deref(),
            args.section_body_file.as_deref(),
        ) {
            (Some(_), Some(_)) => {
                return Err(AppError::invalid_input(
                    "use either --section-body or --section-body-file",
                ));
            }
            (Some(text), None) => text.to_owned(),
            (None, Some(file)) => read_source(file)?,
            (None, None) => {
                return Err(AppError::invalid_input(
                    "--replace-section requires --section-body or --section-body-file",
                ));
            }
        };
        return Ok(Some(BodyChange::ReplaceSection {
            heading: args.replace_section.clone().unwrap_or_default(),
            body: text,
        }));
    }
    if args.section_body.is_some() || args.section_body_file.is_some() {
        return Err(AppError::invalid_input(
            "--section-body and --section-body-file require --replace-section",
        ));
    }
    if let Some(text) = args.body.as_deref() {
        return Ok(Some(BodyChange::Replace(text.to_owned())));
    }
    if let Some(file) = args.body_file.as_deref() {
        return Ok(Some(BodyChange::Replace(read_source(file)?)));
    }
    if let Some(text) = args.append_body.as_deref() {
        return Ok(Some(BodyChange::Append(text.to_owned())));
    }
    if let Some(file) = args.append_body_file.as_deref() {
        return Ok(Some(BodyChange::Append(read_source(file)?)));
    }
    if let Some(file) = args.patch_file.as_deref() {
        return Ok(Some(BodyChange::Patch(read_source(file)?)));
    }
    Ok(None)
}

/// Reads text from a file, or from standard input when `source` is `-`.
pub(super) fn read_source(source: &str) -> Result<String, AppError> {
    if source == "-" {
        return read_stdin();
    }
    let metadata = fs::metadata(source)
        .map_err(|_| AppError::invalid_input("text source must be an existing regular file"))?;
    if !metadata.is_file() {
        return Err(AppError::invalid_input(
            "text source must be an existing regular file",
        ));
    }
    let mut bytes = Vec::new();
    fs::File::open(source)
        .and_then(|file| file.take(MAX_TEXT_BYTES + 1).read_to_end(&mut bytes))
        .map_err(|_| AppError::invalid_input("text source could not be read"))?;
    decode(bytes)
}

fn read_stdin() -> Result<String, AppError> {
    let mut bytes = Vec::new();
    std::io::stdin()
        .lock()
        .take(MAX_TEXT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| AppError::invalid_input("standard input could not be read"))?;
    decode(bytes)
}

fn decode(bytes: Vec<u8>) -> Result<String, AppError> {
    if bytes.len() as u64 > MAX_TEXT_BYTES {
        return Err(AppError::invalid_input("body text exceeds the size limit"));
    }
    String::from_utf8(bytes).map_err(|_| AppError::invalid_input("body text must be valid UTF-8"))
}

/// Resolves repository scope for the selected command's provider domain.
pub(super) fn resolve_repo(
    provider: Option<Provider>,
    explicit_repo: Option<&str>,
) -> Result<String, AppError> {
    let cwd = std::env::current_dir()
        .map_err(|_| AppError::context("could not determine the current directory"))?;
    let provider = provider.ok_or(AppError::context(
        "could not determine a provider; specify --provider",
    ))?;
    config::resolve_domain_context(provider, explicit_repo, Path::new(&cwd))
}
pub(super) fn github_issue_defaults(
    repo: &str,
) -> Result<Option<crate::config::GithubIssueDefaults>, AppError> {
    let cwd = std::env::current_dir()
        .map_err(|_| AppError::context("could not determine the current directory"))?;
    let config = crate::config::load_for_cwd(Path::new(&cwd))?;
    let Some(issue) = config
        .defaults
        .and_then(|defaults| defaults.github)
        .and_then(|github| github.issue)
    else {
        return Ok(None);
    };
    let mut issue = issue;
    if !issue
        .project
        .as_ref()
        .is_some_and(|project| project.repositories.iter().any(|allowed| allowed == repo))
    {
        issue.project = None;
    }
    if issue.project.is_some()
        || !issue.assignees.is_empty()
        || !issue.labels.is_empty()
        || !issue.label_candidates.is_empty()
    {
        Ok(Some(issue))
    } else {
        Ok(None)
    }
}

pub(super) fn github_pr_defaults() -> Result<crate::config::GithubPrDefaults, AppError> {
    let cwd = std::env::current_dir()
        .map_err(|_| AppError::context("could not determine the current directory"))?;
    let config = crate::config::load_for_cwd(Path::new(&cwd))?;
    Ok(config
        .defaults
        .and_then(|defaults| defaults.github)
        .and_then(|github| github.pr)
        .unwrap_or_default())
}
