use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::domain::AppError;
use crate::process::runner::{self, ProcessError, ProcessOutput};

use super::GitLabIssues;

#[derive(Deserialize)]
struct ProjectDetails {
    default_branch: Option<String>,
    issue_branch_template: Option<String>,
}

#[derive(Deserialize)]
struct Branch {
    name: String,
}

/// Create a remote branch whose IID prefix makes GitLab link it to the issue.
pub(super) fn create(
    provider: &GitLabIssues,
    number: u64,
    base: Option<&str>,
    slug: Option<&str>,
    checkout: bool,
) -> Result<String, AppError> {
    let checkout_context = if checkout {
        Some(checkout_context(provider)?)
    } else {
        None
    };
    let issue = provider.show(number)?;
    let project = if base.is_none() || slug.is_none() {
        Some(project_details(provider)?)
    } else {
        None
    };
    let branch = if let Some(slug) = slug {
        issue_branch_name(number, slug)
    } else {
        let project = project.as_ref().ok_or_else(AppError::provider_response)?;
        default_branch_name(provider, number, &issue.title, project)?
    };
    let args = branch_creation_args(provider, &branch, base, project.as_ref())?;
    let response = provider.run_glab_mutation(&args, None)?;
    let created: Branch =
        serde_json::from_slice(&response).map_err(|_| AppError::gitlab_write_uncertain())?;
    if created.name != branch {
        return Err(AppError::gitlab_write_uncertain());
    }
    if let Some((root, remote)) = checkout_context {
        if checkout_branch(&root, &remote, &branch).is_err() {
            return Err(AppError::partial_success(
                serde_json::json!({"issue": number, "branch": branch}),
                &["remote branch created".to_owned()],
                &["local checkout".to_owned()],
            ));
        }
    }
    Ok(branch)
}

fn branch_creation_args(
    provider: &GitLabIssues,
    branch: &str,
    base: Option<&str>,
    project: Option<&ProjectDetails>,
) -> Result<Vec<String>, AppError> {
    let reference = base
        .or_else(|| project.and_then(|project| project.default_branch.as_deref()))
        .filter(|branch| !branch.is_empty())
        .ok_or_else(|| {
            AppError::invalid_input("a base branch is required for an empty GitLab repository")
        })?;
    let endpoint = project_endpoint(provider, "/repository/branches")?;
    Ok(vec![
        "api".to_owned(),
        "--hostname".to_owned(),
        provider.host.clone(),
        "--method".to_owned(),
        "POST".to_owned(),
        endpoint,
        "--raw-field".to_owned(),
        format!("branch={branch}"),
        "--raw-field".to_owned(),
        format!("ref={reference}"),
    ])
}

/// Branches whose names use GitLab's issue-IID prefix convention.
pub(super) fn list(provider: &GitLabIssues, number: u64) -> Result<Vec<String>, AppError> {
    let endpoint = project_endpoint(provider, "/repository/branches")?;
    let prefix = format!("{number}-");
    let args = vec![
        "api".to_owned(),
        "--hostname".to_owned(),
        provider.host.clone(),
        "--method".to_owned(),
        "GET".to_owned(),
        endpoint,
        "--field".to_owned(),
        format!("search={prefix}"),
        "--field".to_owned(),
        "per_page=100".to_owned(),
        "--paginate".to_owned(),
    ];
    let response = provider.run_glab(&args)?;
    let branches: Vec<Branch> =
        serde_json::from_slice(&response).map_err(|_| AppError::provider_response())?;
    Ok(branches
        .into_iter()
        .map(|branch| branch.name)
        .filter(|name| name.starts_with(&prefix))
        .collect())
}

fn project_details(provider: &GitLabIssues) -> Result<ProjectDetails, AppError> {
    let endpoint = project_endpoint(provider, "")?;
    let args = [
        "api".to_owned(),
        "--hostname".to_owned(),
        provider.host.clone(),
        endpoint,
    ];
    let response = provider.run_glab(&args)?;
    serde_json::from_slice(&response).map_err(|_| AppError::provider_response())
}

fn default_branch_name(
    provider: &GitLabIssues,
    number: u64,
    title: &str,
    project: &ProjectDetails,
) -> Result<String, AppError> {
    let prefix = format!("{number}-");
    let template = project
        .issue_branch_template
        .as_deref()
        .unwrap_or("%{id}-%{title}");
    let title_slug = slugify_title(title);
    let mut name = template
        .replace("%{id}", &number.to_string())
        .replace("%{title}", &title_slug);
    if name.contains("%{branch_creator}") {
        name = name.replace("%{branch_creator}", &current_username(provider)?);
    }
    if name.contains("%{") || name.trim().is_empty() {
        return Err(AppError::provider_response());
    }
    if name.starts_with(&prefix) {
        Ok(name)
    } else {
        Ok(format!("{prefix}{name}"))
    }
}

fn current_username(provider: &GitLabIssues) -> Result<String, AppError> {
    let args = [
        "api".to_owned(),
        "--hostname".to_owned(),
        provider.host.clone(),
        "user".to_owned(),
    ];
    let response = provider.run_glab(&args)?;
    let value: serde_json::Value =
        serde_json::from_slice(&response).map_err(|_| AppError::provider_response())?;
    value
        .get("username")
        .and_then(serde_json::Value::as_str)
        .filter(|username| !username.is_empty())
        .map(str::to_owned)
        .ok_or_else(AppError::provider_response)
}

fn project_endpoint(provider: &GitLabIssues, suffix: &str) -> Result<String, AppError> {
    let url = reqwest::Url::parse(&provider.repo_url)
        .map_err(|_| AppError::invalid_input("GitLab repository URL is invalid"))?;
    let path = url
        .path()
        .trim_start_matches('/')
        .strip_suffix(".git")
        .unwrap_or_else(|| url.path().trim_start_matches('/'));
    if path.is_empty() {
        return Err(AppError::invalid_input("GitLab repository URL is invalid"));
    }
    Ok(format!("projects/{}{suffix}", path.replace('/', "%2F")))
}

fn issue_branch_name(number: u64, slug: &str) -> String {
    let prefix = format!("{number}-");
    if slug.starts_with(&prefix) {
        slug.to_owned()
    } else {
        format!("{prefix}{slug}")
    }
}

fn slugify_title(title: &str) -> String {
    let mut slug = String::new();
    let mut separator = false;
    for character in title.chars() {
        if character.is_ascii_alphanumeric() {
            if separator && !slug.is_empty() {
                slug.push('-');
            }
            slug.push(character.to_ascii_lowercase());
            separator = false;
        } else {
            separator = true;
        }
    }
    if slug.is_empty() {
        "issue".to_owned()
    } else {
        slug
    }
}

fn checkout_context(provider: &GitLabIssues) -> Result<(PathBuf, String), AppError> {
    let cwd = std::env::current_dir()
        .map_err(|_| AppError::context("could not determine current directory"))?;
    let root = crate::config::repository_root(&cwd)
        .map_err(|_| AppError::invalid_input("checkout requires a Git worktree"))?;
    let output = run_git(&["remote".to_owned()], &root)
        .map_err(|_| AppError::invalid_input("checkout requires a matching GitLab remote"))?;
    if !output.success {
        return Err(AppError::invalid_input(
            "checkout requires a matching GitLab remote",
        ));
    }
    let expected = normalized_remote(&provider.repo_url)
        .ok_or_else(|| AppError::invalid_input("GitLab repository URL is invalid"))?;
    for remote in String::from_utf8_lossy(&output.stdout).lines() {
        let remote = remote.trim();
        if remote.is_empty() {
            continue;
        }
        let args = vec!["remote".to_owned(), "get-url".to_owned(), remote.to_owned()];
        let urls = run_git(&args, &root)
            .map_err(|_| AppError::invalid_input("checkout requires a matching GitLab remote"))?;
        if !urls.success {
            continue;
        }
        if String::from_utf8_lossy(&urls.stdout)
            .lines()
            .any(|url| normalized_remote(url.trim()).as_ref() == Some(&expected))
        {
            return Ok((root, remote.to_owned()));
        }
    }
    Err(AppError::invalid_input(
        "checkout requires a remote for the selected GitLab project",
    ))
}

fn normalized_remote(value: &str) -> Option<(String, String)> {
    let (host, path) = if let Ok(url) = reqwest::Url::parse(value) {
        (url.host_str()?.to_ascii_lowercase(), url.path().to_owned())
    } else {
        let (user_host, path) = value.rsplit_once(':')?;
        (
            user_host.rsplit('@').next()?.to_ascii_lowercase(),
            path.to_owned(),
        )
    };
    let path = path.trim_start_matches('/').trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    if path.is_empty() {
        None
    } else {
        Some((host, path.to_owned()))
    }
}

fn checkout_branch(root: &Path, remote: &str, branch: &str) -> Result<(), AppError> {
    let fetch_args = vec![
        "fetch".to_owned(),
        "--no-tags".to_owned(),
        remote.to_owned(),
        branch.to_owned(),
    ];
    let fetched = run_git(&fetch_args, root)?;
    if !fetched.success {
        return Err(AppError::context("could not fetch the GitLab branch"));
    }
    let reference = format!("refs/heads/{branch}");
    let exists_args = vec![
        "show-ref".to_owned(),
        "--verify".to_owned(),
        "--quiet".to_owned(),
        reference,
    ];
    let exists = run_git(&exists_args, root)?;
    let switch_args = if exists.success {
        vec!["switch".to_owned(), branch.to_owned()]
    } else {
        vec![
            "switch".to_owned(),
            "--track".to_owned(),
            "--create".to_owned(),
            branch.to_owned(),
            format!("{remote}/{branch}"),
        ]
    };
    let switched = run_git(&switch_args, root)?;
    if switched.success {
        Ok(())
    } else {
        Err(AppError::context("could not check out the GitLab branch"))
    }
}

fn run_git(args: &[String], cwd: &Path) -> Result<ProcessOutput, AppError> {
    runner::run("git", args, None, Some(cwd)).map_err(|error| match error {
        ProcessError::NotFound => AppError::dependency(),
        ProcessError::Timeout => AppError::timeout(),
        ProcessError::OutputLimit => AppError::output_limit(),
        ProcessError::Io => AppError::context("the Git operation could not be completed"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rf_wi_13_remote_default_reference_is_a_literal_raw_field() {
        let provider = GitLabIssues::new("https://gitlab.example.test/group/repo".to_owned())
            .expect("fixture provider");
        let project: ProjectDetails = serde_json::from_str(
            r#"{"default_branch":"@synthetic-private-config","issue_branch_template":null}"#,
        )
        .expect("fixture project response");
        let args = branch_creation_args(&provider, "12-fix", None, Some(&project))
            .expect("branch creation arguments");

        assert_eq!(
            args,
            [
                "api",
                "--hostname",
                "gitlab.example.test",
                "--method",
                "POST",
                "projects/group%2Frepo/repository/branches",
                "--raw-field",
                "branch=12-fix",
                "--raw-field",
                "ref=@synthetic-private-config",
            ]
            .map(str::to_owned)
        );
    }

    #[test]
    fn rf_wi_13_explicit_reference_and_branch_remain_literal_at_values() {
        let provider = GitLabIssues::new("https://gitlab.example.test/group/repo".to_owned())
            .expect("fixture provider");
        let project = ProjectDetails {
            default_branch: Some("main".to_owned()),
            issue_branch_template: None,
        };
        let args = branch_creation_args(&provider, "@synthetic-branch", Some("@-"), Some(&project))
            .expect("literal branch creation arguments");

        assert_eq!(
            &args[6..],
            [
                "--raw-field",
                "branch=@synthetic-branch",
                "--raw-field",
                "ref=@-",
            ]
            .map(str::to_owned)
        );
    }

    #[test]
    fn rf_wi_13_missing_or_empty_base_still_fails_closed() {
        let provider = GitLabIssues::new("https://gitlab.example.test/group/repo".to_owned())
            .expect("fixture provider");
        let project = ProjectDetails {
            default_branch: Some(String::new()),
            issue_branch_template: None,
        };
        for (base, project) in [(None, None), (None, Some(&project)), (Some(""), None)] {
            assert_eq!(
                branch_creation_args(&provider, "12-fix", base, project)
                    .expect_err("missing base")
                    .code,
                "invalid_input"
            );
        }
    }
}
