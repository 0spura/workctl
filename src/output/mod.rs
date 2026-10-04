pub mod json;
pub mod text;
use crate::cli::OutputFormat;

use serde::Serialize;

use crate::domain::{AppError, CheckRun, Issue, IssueSummary, PullRequest, PullRequestSummary};

#[derive(Serialize)]
#[serde(untagged)]
pub enum SuccessOutput {
    Issue(Issue),
    Issues(Vec<IssueSummary>),
    PullRequest(PullRequest),
    PullRequests(Vec<PullRequestSummary>),
    Checks(Vec<CheckRun>),
    Diff {
        number: u64,
        diff: String,
    },
    Merge {
        number: u64,
        method: Option<String>,
        auto: bool,
    },
    Review {
        number: u64,
        event: String,
    },
    Ready {
        number: u64,
        draft: bool,
    },
    State {
        number: u64,
        state: String,
    },
}

pub fn write(format: OutputFormat, output: &SuccessOutput) -> Result<(), AppError> {
    match format {
        OutputFormat::Json => json::write_success(output),
        OutputFormat::Text => text::write(output),
    }
}
