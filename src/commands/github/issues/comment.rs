use crate::cli::GlobalArgs;
use crate::cli::common::parse_non_blank;
use crate::cli::github::issues::CommentArgs;
use crate::domain::AppError;
use crate::output::SuccessOutput;
use crate::providers::WorkItemProvider;

use super::shared;

pub(super) fn execute(globals: &GlobalArgs, args: CommentArgs) -> Result<SuccessOutput, AppError> {
    let source = crate::commands::support::optional_text(
        args.body.as_deref(),
        args.body_file.as_deref(),
        "choose either --body or --body-file",
    )?;
    let body = source.ok_or_else(|| {
        AppError::invalid_input("comment text is required via --body or --body-file")
    })?;
    let body = parse_non_blank(&body).map_err(AppError::invalid_input)?;
    let provider = shared::provider(globals)?;
    provider.comment(args.number.0, &body)?;
    Ok(SuccessOutput::Comment {
        number: args.number.0,
        target: "issue".to_owned(),
    })
}
