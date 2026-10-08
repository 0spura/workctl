use crate::cli::github::prs::ReviewArgs;
use crate::cli::GlobalArgs;
use crate::domain::AppError;
use crate::output::SuccessOutput;
use crate::providers::{PullRequestProvider, ReviewEvent};

use crate::commands::support;

use super::shared;

pub(super) fn execute(globals: &GlobalArgs, args: ReviewArgs) -> Result<SuccessOutput, AppError> {
    let event = review_event(&args);
    let body = support::optional_text(
        args.body.as_deref(),
        args.body_file.as_deref(),
        "use either --body or --body-file",
    )?;
    let provider = shared::provider(globals)?;
    provider.review(args.number.0, event, body.as_deref())?;
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
