use std::io::{self, Write};

use serde_json::Value;

use crate::cli::OutputFormat;
use crate::domain::AppError;

use super::text::write_safe;

pub fn write(format: OutputFormat, error: &AppError) {
    match format {
        OutputFormat::Json => super::json::write_error(error),
        OutputFormat::Text => {
            let stderr = io::stderr();
            let _ = write_to(&mut stderr.lock(), error);
        }
    }
}

fn write_to(writer: &mut impl Write, error: &AppError) -> Result<(), AppError> {
    write!(writer, "{}: ", error.code).map_err(|_| AppError::output())?;
    write_safe(writer, error.message, false)?;
    if let Some(details) = error
        .details
        .as_ref()
        .filter(|_| error.code == "partial_success")
    {
        write_details(writer, details)?;
    }
    writeln!(writer).map_err(|_| AppError::output())
}

fn write_details(writer: &mut impl Write, details: &Value) -> Result<(), AppError> {
    if let Some(resource) = details.get("resource") {
        write_resource(writer, resource)?;
    }
    for key in ["completed", "pending"] {
        if let Some(operations) = details.get(key).and_then(Value::as_array) {
            write!(writer, "; {key}:").map_err(|_| AppError::output())?;
            for (index, operation) in operations.iter().filter_map(Value::as_str).enumerate() {
                write!(writer, "{}", if index == 0 { " " } else { ", " })
                    .map_err(|_| AppError::output())?;
                write_reference(writer, operation)?;
            }
        }
    }
    let failure = details.get("failure").or_else(|| {
        details
            .get("resource")
            .and_then(|resource| resource.get("failure"))
    });
    if let Some(code) = failure
        .and_then(|failure| failure.get("code"))
        .and_then(Value::as_str)
    {
        write!(writer, "; failure: ").map_err(|_| AppError::output())?;
        write_safe(writer, code, false)?;
    }
    if let Some(nested) = failure
        .and_then(|failure| failure.get("details"))
        .filter(|v| !v.is_null())
    {
        write_details(writer, nested)?;
    }
    Ok(())
}

fn write_resource(writer: &mut impl Write, resource: &Value) -> Result<(), AppError> {
    write!(writer, "; resource: ").map_err(|_| AppError::output())?;
    if let Some(url) = resource.get("url").and_then(Value::as_str) {
        write_reference(writer, url)?;
    } else if let Some(number) = resource.get("number").and_then(Value::as_u64) {
        write!(writer, "#{number}").map_err(|_| AppError::output())?;
    } else if let Some(failed) = resource.get("failed").and_then(Value::as_str) {
        write_reference(writer, failed)?;
    } else {
        write_safe(writer, "identity unconfirmed", false)?;
    }
    if let Some(target) = resource.get("target").and_then(Value::as_str) {
        write!(writer, "; target: ").map_err(|_| AppError::output())?;
        write_reference(writer, target)?;
    }
    Ok(())
}

fn write_reference(writer: &mut impl Write, value: &str) -> Result<(), AppError> {
    if let Some(path) = value.strip_prefix("https://github.com/") {
        let mut parts = path.split('/');
        match (
            parts.next(),
            parts.next(),
            parts.next(),
            parts.next(),
            parts.next(),
        ) {
            (Some(owner), Some(repo), Some("issues" | "pull"), Some(number), None)
                if !owner.is_empty() && !repo.is_empty() && number.parse::<u64>().is_ok() =>
            {
                write_safe(writer, owner, false)?;
                write!(writer, "/").map_err(|_| AppError::output())?;
                write_safe(writer, repo, false)?;
                write!(writer, "#{number}").map_err(|_| AppError::output())?;
                return Ok(());
            }
            (Some("orgs" | "users"), Some(owner), Some("projects"), Some(number), None)
                if !owner.is_empty() && number.parse::<u64>().is_ok() =>
            {
                write!(writer, "project ").map_err(|_| AppError::output())?;
                write_safe(writer, owner, false)?;
                write!(writer, "/{number}").map_err(|_| AppError::output())?;
                return Ok(());
            }
            _ => {}
        }
    }
    write_safe(writer, value, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rf_out_1_partial_failure_retains_recovery_without_resource_body() {
        let error = AppError::partial_success(
            serde_json::json!({"type": "issue", "number": 7,
                "url": "https://github.com/owner/repo/issues/7", "body": "submitted private body"}),
            &["issue edit".to_owned()],
            &["issue readback".to_owned()],
        );
        let mut output = Vec::new();
        write_to(&mut output, &error).unwrap();
        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("owner/repo#7"));
        assert!(output.contains("completed: issue edit"));
        assert!(output.contains("pending: issue readback"));
        assert!(!output.contains("submitted private body"));
        assert!(!output.contains("https://"));
    }

    #[test]
    fn rf_out_1_batch_failure_keeps_cross_repository_identity_and_escapes_controls() {
        let error = AppError::partial_success(
            serde_json::json!({"type": "issue_batch", "failed": "https://github.com/other/repo/issues/8"}),
            &["https://github.com/owner/repo/issues/7".to_owned()],
            &[
                "https://github.com/other/repo/issues/8".to_owned(),
                "field\u{1b}[31m".to_owned(),
            ],
        );
        let mut output = Vec::new();
        write_to(&mut output, &error).unwrap();
        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("owner/repo#7"));
        assert!(output.contains("other/repo#8"));
        assert!(!output.contains('\u{1b}'));
        assert!(output.contains("\\u{1b}[31m"));
    }

    #[test]
    fn rf_out_1_batch_failure_preserves_partial_progress_of_failed_item() {
        let nested = AppError::partial_success(
            serde_json::json!({"number": 8}),
            &["issue edit".to_owned()],
            &["transition comment".to_owned(), "issue readback".to_owned()],
        );
        let error = AppError::partial_success(
            serde_json::json!({"type": "issue_batch",
                "failed": "https://github.com/owner/repo/issues/8",
                "failure": {"code": nested.code, "details": nested.details}}),
            &["https://github.com/owner/repo/issues/7".to_owned()],
            &["https://github.com/owner/repo/issues/8".to_owned()],
        );
        let mut output = Vec::new();
        write_to(&mut output, &error).unwrap();
        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("completed: owner/repo#7"));
        assert!(output.contains("completed: issue edit"));
        assert!(output.contains("pending: transition comment, issue readback"));
    }
}
