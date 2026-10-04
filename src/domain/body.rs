use crate::domain::AppError;

/// Appends text as a new trailing block, keeping exactly one blank-line-free separator.
pub fn append(body: &str, text: &str) -> String {
    if text.is_empty() {
        return body.to_owned();
    }
    let trimmed = body.trim_end_matches('\n');
    if trimmed.trim().is_empty() {
        text.to_owned()
    } else {
        format!("{trimmed}\n{text}")
    }
}

/// Replaces the content of the ATX section named by `heading`, leaving other sections untouched.
///
/// The section spans from the line after the heading to the next heading of the same or higher
/// level, or to the end of the body. An empty replacement clears the section content.
pub fn replace_section(body: &str, heading: &str, text: &str) -> Result<String, AppError> {
    let heading = heading.trim();
    let level = atx_level(heading).ok_or(AppError::invalid_input(
        "section heading must be an ATX heading",
    ))?;
    let lines: Vec<&str> = body.split('\n').collect();
    let start = lines
        .iter()
        .position(|line| line.trim() == heading)
        .ok_or(AppError::section_not_found())?;
    let end = (start + 1..lines.len())
        .find(|&index| atx_level(lines[index]).is_some_and(|next| next <= level))
        .unwrap_or(lines.len());

    let mut result: Vec<&str> = Vec::with_capacity(lines.len());
    result.extend_from_slice(&lines[..=start]);
    if !text.is_empty() {
        result.extend(text.split('\n'));
    }
    if end < lines.len() && result.last().is_some_and(|line| !line.is_empty()) {
        result.push("");
    }
    result.extend_from_slice(&lines[end..]);
    Ok(result.join("\n"))
}

fn atx_level(line: &str) -> Option<usize> {
    let trimmed = line.trim_start();
    let hashes = trimmed
        .chars()
        .take_while(|character| *character == '#')
        .count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    let rest = &trimmed[hashes..];
    if rest.is_empty() || rest.starts_with(' ') {
        Some(hashes)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{append, replace_section};

    #[test]
    fn append_separates_with_one_newline_and_handles_empty_bodies() {
        assert_eq!(append("first", "second"), "first\nsecond");
        assert_eq!(append("first\n\n", "second"), "first\nsecond");
        assert_eq!(append("", "second"), "second");
        assert_eq!(append("   ", "second"), "second");
        assert_eq!(append("first", ""), "first");
    }

    #[test]
    fn replaces_only_the_named_section() {
        let body = "# Title\n\nintro\n\n## Acceptance\n\nold acceptance\n\n### Note\n\nkeep\n\n## Other\n\nkeep other";
        let updated =
            replace_section(body, "## Acceptance", "new acceptance").expect("replace section");
        assert_eq!(
            updated,
            "# Title\n\nintro\n\n## Acceptance\nnew acceptance\n\n## Other\n\nkeep other"
        );
        assert!(updated.contains("keep other"));
    }

    #[test]
    fn replaces_a_trailing_section_and_can_clear_it() {
        let body = "## First\n\nfirst\n\n## Last\n\nlast content";
        assert_eq!(
            replace_section(body, "## Last", "replaced").expect("trailing section"),
            "## First\n\nfirst\n\n## Last\nreplaced"
        );
        assert_eq!(
            replace_section(body, "## Last", "").expect("clear section"),
            "## First\n\nfirst\n\n## Last"
        );
    }

    #[test]
    fn rejects_missing_or_invalid_headings() {
        let body = "## Only\n\ncontent";
        assert_eq!(
            replace_section(body, "## Missing", "x").unwrap_err().code,
            "section_not_found"
        );
        assert_eq!(
            replace_section(body, "Acceptance", "x").unwrap_err().code,
            "invalid_input"
        );
    }
}
