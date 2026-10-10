use crate::cli::GlobalArgs;
use crate::cli::github::prs::ReviewArgs;
use crate::domain::AppError;
use crate::output::SuccessOutput;
use crate::providers::{InlineReviewComment, PullRequestProvider, ReviewEvent, ReviewSide};

use crate::commands::support;

use super::shared;

/// Aggregate bound for one review payload: inline paths and bodies plus the summary.
const MAX_REVIEW_BYTES: usize = 1024 * 1024;

pub(super) fn execute(globals: &GlobalArgs, args: ReviewArgs) -> Result<SuccessOutput, AppError> {
    let event = review_event(&args);
    let requests = inline_requests(&args)?;
    reject_multiple_stdin_readers(&args, &requests)?;
    let summary = support::optional_text(
        args.body.as_deref(),
        args.body_file.as_deref(),
        "use either --body or --body-file",
    )?;
    let comments = inline_comments(requests, summary.as_deref())?;
    let has_summary = summary.as_deref().is_some_and(has_text);
    if !comments.is_empty() && requires_summary(event) && !has_summary {
        return Err(AppError::invalid_input(
            "an inline --comment or --request-changes review requires a nonblank --body or --body-file",
        ));
    }
    let provider = shared::provider(globals)?;
    provider.review(args.number.0, event, summary.as_deref(), &comments)?;
    Ok(SuccessOutput::Review {
        number: args.number.0,
        event: event.as_str().to_owned(),
    })
}

fn review_event(args: &ReviewArgs) -> ReviewEvent {
    if args.approve {
        ReviewEvent::Approve
    } else if args.request_changes {
        ReviewEvent::RequestChanges
    } else {
        ReviewEvent::Comment
    }
}

/// The GitHub review API requires an authored summary for `COMMENT` and `REQUEST_CHANGES`.
fn requires_summary(event: ReviewEvent) -> bool {
    matches!(event, ReviewEvent::Comment | ReviewEvent::RequestChanges)
}

fn has_text(value: &str) -> bool {
    !value.trim().is_empty()
}

/// One `--inline` or `--inline-file` occurrence: a validated location and its body source.
struct InlineRequest {
    path: String,
    line: u64,
    side: ReviewSide,
    source: InlineSource,
}

enum InlineSource {
    Text(String),
    File(String),
}

impl InlineSource {
    fn is_stdin(&self) -> bool {
        matches!(self, Self::File(file) if file == "-")
    }
}

/// Parses every inline flag pair; clap guarantees exactly two values per occurrence.
fn inline_requests(args: &ReviewArgs) -> Result<Vec<InlineRequest>, AppError> {
    let mut requests = Vec::with_capacity(args.inline.len() + args.inline_file.len());
    for pair in args.inline.chunks_exact(2) {
        let [location, text] = pair else {
            unreachable!("clap collects two values per --inline occurrence");
        };
        let (path, line, side) = parse_location(location)?;
        requests.push(InlineRequest {
            path,
            line,
            side,
            source: InlineSource::Text(text.clone()),
        });
    }
    for pair in args.inline_file.chunks_exact(2) {
        let [location, file] = pair else {
            unreachable!("clap collects two values per --inline-file occurrence");
        };
        let (path, line, side) = parse_location(location)?;
        requests.push(InlineRequest {
            path,
            line,
            side,
            source: InlineSource::File(file.clone()),
        });
    }
    Ok(requests)
}

/// Parses `PATH:LINE[:left|right]` from the right, so paths may contain colons.
fn parse_location(value: &str) -> Result<(String, u64, ReviewSide), AppError> {
    let (remainder, side) = match value.rsplit_once(':') {
        Some((remainder, "left")) => (remainder, ReviewSide::Left),
        Some((remainder, "right")) => (remainder, ReviewSide::Right),
        _ => (value, ReviewSide::Right),
    };
    let (path, line) = remainder
        .rsplit_once(':')
        .filter(|(path, _)| !path.is_empty())
        .ok_or_else(|| AppError::invalid_input("location must be PATH:LINE[:left|right]"))?;
    let line = line
        .parse::<u64>()
        .ok()
        .filter(|line| *line > 0)
        .ok_or_else(|| AppError::invalid_input("location line must be a positive number"))?;
    Ok((path.to_owned(), line, side))
}

/// `--body-file -` and any `--inline-file LOCATION -` read the same standard input, so at most
/// one source may consume it.
fn reject_multiple_stdin_readers(
    args: &ReviewArgs,
    requests: &[InlineRequest],
) -> Result<(), AppError> {
    let mut readers = requests
        .iter()
        .filter(|request| request.source.is_stdin())
        .count();
    if args.body_file.as_deref() == Some("-") {
        readers += 1;
    }
    if readers > 1 {
        return Err(AppError::invalid_input(
            "only one of --body-file and --inline-file may read standard input",
        ));
    }
    Ok(())
}

/// Resolves every inline body; each body must be nonblank text.
fn inline_comments(
    requests: Vec<InlineRequest>,
    summary: Option<&str>,
) -> Result<Vec<InlineReviewComment>, AppError> {
    let mut bytes = summary.map_or(0, str::len);
    if bytes > MAX_REVIEW_BYTES {
        return Err(AppError::invalid_input(
            "the review text exceeds the size limit",
        ));
    }
    let mut comments = Vec::with_capacity(requests.len());
    for request in requests {
        bytes = bytes.saturating_add(request.path.len());
        if bytes > MAX_REVIEW_BYTES {
            return Err(AppError::invalid_input(
                "the review text exceeds the size limit",
            ));
        }
        let body = match request.source {
            InlineSource::Text(text) => text,
            InlineSource::File(file) => support::read_source(&file)?,
        };
        if !has_text(&body) {
            return Err(AppError::invalid_input(
                "inline review text must not be blank",
            ));
        }
        bytes = bytes.saturating_add(body.len());
        if bytes > MAX_REVIEW_BYTES {
            return Err(AppError::invalid_input(
                "the review text exceeds the size limit",
            ));
        }
        comments.push(InlineReviewComment {
            path: request.path,
            line: request.line,
            side: request.side,
            body,
        });
    }
    Ok(comments)
}

#[cfg(test)]
mod tests {
    use super::parse_location;
    use crate::providers::ReviewSide;

    fn assert_location(value: &str, path: &str, line: u64, side: ReviewSide) {
        let (parsed_path, parsed_line, parsed_side) =
            parse_location(value).expect("valid location");
        assert_eq!(parsed_path, path, "{value}");
        assert_eq!(parsed_line, line, "{value}");
        let side_matches = matches!(
            (parsed_side, side),
            (ReviewSide::Left, ReviewSide::Left) | (ReviewSide::Right, ReviewSide::Right)
        );
        assert!(side_matches, "{value}");
    }

    #[test]
    fn rf_pr_6_location_defaults_to_the_right_side_and_keeps_colons_in_paths() {
        assert_location("src/lib.rs:10", "src/lib.rs", 10, ReviewSide::Right);
        assert_location("src/lib.rs:10:left", "src/lib.rs", 10, ReviewSide::Left);
        assert_location("src/a:b.rs:12:right", "src/a:b.rs", 12, ReviewSide::Right);
        assert_location("weird:left:3", "weird:left", 3, ReviewSide::Right);
    }

    #[test]
    fn rf_pr_6_malformed_locations_are_rejected() {
        for location in [
            "src/lib.rs",
            "",
            ":10",
            "src/lib.rs:",
            "src/lib.rs:0",
            "src/lib.rs:ten",
        ] {
            assert!(
                parse_location(location).is_err(),
                "{location} must be rejected"
            );
        }
    }
}
