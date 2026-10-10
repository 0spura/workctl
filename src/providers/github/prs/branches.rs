use serde::Deserialize;

use crate::domain::AppError;
use crate::providers::github::prs::GitHubPulls;

/// The reference endpoints of a pull request, used to decide which branch may be deleted.
#[derive(Deserialize)]
struct PullReferences {
    head: Reference,
    base: Reference,
}

#[derive(Deserialize)]
struct Reference {
    #[serde(rename = "ref")]
    name: String,
    /// Absent when the source repository of a fork pull request no longer exists.
    repo: Option<Repository>,
}

#[derive(Deserialize)]
struct Repository {
    full_name: String,
}

/// Deletes the pull request's remote head branch, leaving every local branch untouched.
///
/// A branch that is already gone counts as deleted: GitHub answers a deletion of a missing ref with
/// 422 `Reference does not exist`, which is the state this call exists to reach.
pub(super) fn delete_remote_branch(provider: &GitHubPulls, number: u64) -> Result<(), AppError> {
    let Some(branch) = owned_head_branch(provider, number)? else {
        return Ok(());
    };
    let args = vec![
        "api".to_owned(),
        "--method".to_owned(),
        "DELETE".to_owned(),
        format!(
            "repos/{}/git/refs/heads/{}",
            provider.repo,
            encode_path(&branch)
        ),
    ];
    let output = provider.run_gh_raw(&args, None)?;
    if output.success || reference_is_gone(&output.stderr) {
        return Ok(());
    }
    Err(AppError::github_cli())
}

/// The pull request's head branch, but only when it belongs to this repository.
///
/// A pull request opened from a fork has its head branch in another repository, where this
/// repository may hold an unrelated branch of the same name. That branch is never deleted; the
/// caller has no request to touch it.
fn owned_head_branch(provider: &GitHubPulls, number: u64) -> Result<Option<String>, AppError> {
    let args = vec![
        "api".to_owned(),
        format!("repos/{}/pulls/{number}", provider.repo),
    ];
    let payload = provider.run_gh(&args, None)?;
    let pull: PullReferences =
        serde_json::from_slice(&payload).map_err(|_| AppError::provider_response())?;
    let belongs_here = |reference: &Reference| {
        reference
            .repo
            .as_ref()
            .is_some_and(|repo| repo.full_name.eq_ignore_ascii_case(provider.repo.as_str()))
    };
    if !belongs_here(&pull.head) || !belongs_here(&pull.base) {
        return Ok(None);
    }
    Ok(Some(pull.head.name))
}

/// True when `gh` reports that the ref to delete is not there.
fn reference_is_gone(stderr: &[u8]) -> bool {
    String::from_utf8_lossy(stderr).contains("Reference does not exist")
}

/// Percent-encodes a branch name for use as one API path segment.
///
/// Git allows characters such as `#` in a ref name, and an unencoded `#` would truncate the path
/// at the URL fragment rather than name the branch.
fn encode_path(branch: &str) -> String {
    let mut encoded = String::with_capacity(branch.len());
    for byte in branch.bytes() {
        if byte.is_ascii_alphanumeric() || b"._-/".contains(&byte) {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_every_character_a_path_cannot_carry() {
        assert_eq!(encode_path("feature/branch-1.2_x"), "feature/branch-1.2_x");
        assert_eq!(encode_path("bug#42"), "bug%2342");
        assert_eq!(encode_path("fix a b"), "fix%20a%20b");
        assert_eq!(encode_path("café"), "caf%C3%A9");
    }

    #[test]
    fn reads_the_reference_owner_of_each_side() {
        let payload = br#"{"head":{"ref":"feature-x","repo":{"full_name":"owner/repo"}},
            "base":{"ref":"main","repo":{"full_name":"owner/repo"}}}"#;
        let pull: PullReferences = serde_json::from_slice(payload).expect("parse references");
        assert_eq!(pull.head.name, "feature-x");
        assert_eq!(
            pull.base.repo.expect("base repository").full_name,
            "owner/repo"
        );
    }
}
