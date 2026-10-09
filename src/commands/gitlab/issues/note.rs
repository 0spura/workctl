use crate::cli::GlobalArgs;
use crate::cli::gitlab::issues::NoteArgs;
use crate::domain::AppError;
use crate::output::SuccessOutput;

use super::shared;

pub(super) fn execute(globals: &GlobalArgs, args: NoteArgs) -> Result<SuccessOutput, AppError> {
    let provider = shared::provider(globals)?;
    provider.add_note(args.number.0, &args.message)?;
    Ok(SuccessOutput::Comment {
        number: args.number.0,
        target: "issue".to_owned(),
    })
}
