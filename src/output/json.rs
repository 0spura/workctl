use std::io::{self, Write};

use serde::Serialize;

use crate::domain::AppError;

pub fn write_success<T: Serialize>(value: &T) -> Result<(), AppError> {
    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    serde_json::to_writer(&mut stdout, value).map_err(|_| AppError::output())?;
    stdout.write_all(b"\n").map_err(|_| AppError::output())
}

pub fn write_error(error: &AppError) {
    let stderr = io::stderr();
    let mut stderr = stderr.lock();
    let mut value = serde_json::json!({
        "code": error.code,
        "message": error.message,
    });
    if let Some(details) = &error.details {
        value["details"] = details.clone();
    }
    let _ = serde_json::to_writer(&mut stderr, &value);
    let _ = stderr.write_all(b"\n");
}
