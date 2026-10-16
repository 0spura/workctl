#[derive(Debug, Clone)]
pub struct AppError {
    pub code: &'static str,
    pub message: &'static str,
    pub details: Option<serde_json::Value>,
    /// Sanitized, bounded first line of a failing CLI's stderr, so the caller can act on it.
    pub cause: Option<String>,
    pub(crate) partial_event: Option<PartialEvent>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct PartialEvent {
    pub resource: &'static str,
    pub number: u64,
    pub operation: &'static str,
}

impl AppError {
    pub const fn new(code: &'static str, message: &'static str) -> Self {
        Self {
            code,
            message,
            details: None,
            partial_event: None,
            cause: None,
        }
    }

    pub fn partial_success(
        resource: serde_json::Value,
        completed: &[String],
        pending: &[String],
    ) -> Self {
        Self {
            code: "partial_success",
            message: "the operation completed partially; inspect the reported resource before retrying",
            details: Some(serde_json::json!({
                "resource": resource,
                "completed": completed,
                "pending": pending,
            })),
            partial_event: None,
            cause: None,
        }
    }

    pub(crate) fn with_partial_event(mut self, event: PartialEvent) -> Self {
        self.partial_event = Some(event);
        self
    }

    /// Attaches the failing CLI's stderr as a sanitized one-line cause.
    pub(crate) fn with_stderr(mut self, stderr: &[u8]) -> Self {
        self.cause = cli_cause(stderr);
        self
    }

    /// Attaches a pre-sanitized one-line cause.
    pub(crate) fn with_cause(mut self, cause: &str) -> Self {
        self.cause = cli_cause(cause.as_bytes());
        self
    }

    pub const fn invalid_input(message: &'static str) -> Self {
        Self::new("invalid_input", message)
    }

    pub const fn context(message: &'static str) -> Self {
        Self::new("context", message)
    }

    pub const fn config(message: &'static str) -> Self {
        Self::new("config", message)
    }

    pub const fn provider_unsupported() -> Self {
        Self::new(
            "provider_unsupported",
            "the selected provider is not supported",
        )
    }

    pub const fn gitlab_authentication() -> Self {
        Self::new("authentication", "GitLab CLI authentication is required")
    }
    pub const fn gitlab_write_uncertain() -> Self {
        Self::new(
            "gitlab_write_uncertain",
            "the GitLab write may have succeeded but its result could not be confirmed; check the remote resource before retrying",
        )
    }

    pub const fn gitlab_cli() -> Self {
        Self::new("gitlab_cli", "the GitLab CLI operation failed")
    }

    pub const fn dependency() -> Self {
        Self::new("dependency", "a required command is unavailable")
    }

    pub const fn authentication() -> Self {
        Self::new("authentication", "GitHub CLI authentication is required")
    }

    pub const fn github_cli() -> Self {
        Self::new("github_cli", "the GitHub CLI operation failed")
    }

    pub const fn github_write_uncertain() -> Self {
        Self::new(
            "github_write_uncertain",
            "the GitHub write may have succeeded; inspect the remote resource before retrying",
        )
    }

    pub const fn decision_authentication() -> Self {
        Self::new(
            "decision_authentication",
            "a valid decision-model API key is required for the selected hosted model; set DECISION_MODEL_API_KEY or configure decisionModel.apiKey in .workctl.local.json",
        )
    }

    pub const fn decision_config() -> Self {
        Self::new(
            "decision_config",
            "the decision-model configuration is invalid; select a supported model and keep the local base URL on loopback",
        )
    }

    pub const fn decision_model() -> Self {
        Self::new(
            "decision_model",
            "the selected decision model request failed",
        )
    }

    pub const fn decision_response() -> Self {
        Self::new(
            "decision_response",
            "the selected decision model returned invalid label scores",
        )
    }

    pub const fn decision_input_limit() -> Self {
        Self::new(
            "decision_input_limit",
            "the decision-model label request exceeds supported limits",
        )
    }

    pub const fn attachment_create_uncertain() -> Self {
        Self::new(
            "attachment_create_uncertain",
            "GitHub CLI attachment operation failed; the item may have been created. Check GitHub before retrying",
        )
    }

    pub const fn attachment_cli_version() -> Self {
        Self::new(
            "dependency_version",
            "GitHub CLI 2.99.0 or newer is required to attach files",
        )
    }

    pub const fn provider_response() -> Self {
        Self::new(
            "provider_response",
            "the provider CLI returned an invalid response",
        )
    }

    pub const fn relationship_limit() -> Self {
        Self::new(
            "relationship_limit",
            "the issue blocker graph exceeds supported traversal limits",
        )
    }

    pub const fn not_issue() -> Self {
        Self::new("not_issue", "the selected number is not a GitHub issue")
    }

    pub const fn not_pull_request() -> Self {
        Self::new(
            "not_pull_request",
            "the selected number is not a GitHub pull request",
        )
    }

    pub const fn section_not_found() -> Self {
        Self::new(
            "section_not_found",
            "the section heading was not found in the body",
        )
    }

    pub const fn conflict() -> Self {
        Self::new("conflict", "the item changed since it was read")
    }

    pub const fn patch_conflict() -> Self {
        Self::new(
            "patch_conflict",
            "the patch does not apply to the current body",
        )
    }

    pub const fn timeout() -> Self {
        Self::new("timeout", "a required command exceeded its time limit")
    }

    pub const fn output_limit() -> Self {
        Self::new(
            "output_limit",
            "a required command exceeded the output limit",
        )
    }

    pub const fn output() -> Self {
        Self::new("output", "workctl could not write its output")
    }
}

const CAUSE_LIMIT: usize = 300;
const SECRET_PREFIXES: [&str; 8] = [
    "ghp_",
    "gho_",
    "ghu_",
    "ghs_",
    "ghr_",
    "github_pat_",
    "glpat-",
    "gldt-",
];

/// First non-empty stderr line of a failing CLI, bounded and stripped of credentials.
///
/// Control characters are left for the writers to escape. Words that look like tokens, the word
/// after `Bearer`/`token`, and URLs with embedded credentials are replaced with `[redacted]`.
pub(crate) fn cli_cause(stderr: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(stderr);
    let line = text.lines().map(str::trim).find(|line| !line.is_empty())?;
    let mut words = Vec::new();
    let mut redact_next = false;
    for word in line.split_whitespace() {
        let lower = word.to_ascii_lowercase();
        let secret = redact_next
            || SECRET_PREFIXES.iter().any(|prefix| lower.contains(prefix))
            || embeds_credentials(word);
        redact_next = matches!(
            lower.as_str(),
            "bearer" | "token" | "authorization:" | "basic"
        );
        words.push(if secret { "[redacted]" } else { word });
    }
    let joined = words.join(" ");
    let mut cause = String::new();
    for character in joined.chars().take(CAUSE_LIMIT) {
        cause.push(character);
    }
    if joined.chars().count() > CAUSE_LIMIT {
        cause.push('…');
    }
    Some(cause)
}

fn embeds_credentials(word: &str) -> bool {
    word.split_once("://").is_some_and(|(_, rest)| {
        rest.split('/')
            .next()
            .is_some_and(|auth| auth.contains('@'))
    })
}

#[cfg(test)]
mod cause_tests {
    use super::cli_cause;

    #[test]
    fn keeps_first_line_and_redacts_credentials() {
        let cause = cli_cause(
            b"\n  HTTP 403: token ghp_abc123 rejected for https://u:p@host/x Bearer s3cret\nsecond",
        )
        .unwrap();
        assert_eq!(
            cause,
            "HTTP 403: token [redacted] rejected for [redacted] Bearer [redacted]"
        );
    }

    #[test]
    fn bounds_length_and_ignores_blank_stderr() {
        assert!(cli_cause(b"  \n\t").is_none());
        let long = "x".repeat(1000);
        assert_eq!(cli_cause(long.as_bytes()).unwrap().chars().count(), 301);
    }
}
