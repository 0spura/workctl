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

/// An issue a pull-request body closes when the pull request merges.
///
/// GitHub links the pull request to the issue when the body carries a closing keyword next to the
/// reference, and closes the issue when the pull request merges.
#[derive(Debug, Clone)]
pub struct ClosingReference {
    /// `OWNER/REPO` when the issue lives outside the pull request's repository.
    pub repo: Option<String>,
    pub number: u64,
}

impl std::fmt::Display for ClosingReference {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.repo {
            Some(repo) => write!(formatter, "{repo}#{}", self.number),
            None => write!(formatter, "#{}", self.number),
        }
    }
}

/// Repository names are case-insensitive on GitHub, so references compare that way too.
impl PartialEq for ClosingReference {
    fn eq(&self, other: &Self) -> bool {
        if self.number != other.number {
            return false;
        }
        match (&self.repo, &other.repo) {
            (None, None) => true,
            (Some(left), Some(right)) => left.eq_ignore_ascii_case(right),
            _ => false,
        }
    }
}

impl Eq for ClosingReference {}

impl std::str::FromStr for ClosingReference {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        /// Mirrors the canonical reference form: no empty, `.`, or `..` segment, and no character
        /// a GitHub repository name cannot hold.
        fn valid_part(part: &str) -> bool {
            !part.is_empty()
                && part != "."
                && part != ".."
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
        }

        const INVALID: &str = "expected a positive issue number or OWNER/REPO#NUMBER";
        let Some((repo, number)) = value.split_once('#') else {
            return Ok(Self {
                repo: None,
                number: positive_number(value).ok_or(INVALID)?,
            });
        };
        let number = positive_number(number).ok_or(INVALID)?;
        // `#NUMBER` names an issue in the pull request's own repository.
        if repo.is_empty() {
            return Ok(Self { repo: None, number });
        }
        let mut parts = repo.split('/');
        let (owner, name) = (parts.next(), parts.next());
        if parts.next().is_some() {
            return Err(INVALID);
        }
        let (Some(owner), Some(name)) = (owner, name) else {
            return Err(INVALID);
        };
        if !valid_part(owner) || !valid_part(name) {
            return Err(INVALID);
        }
        Ok(Self {
            repo: Some(format!("{owner}/{name}")),
            number,
        })
    }
}

/// Closing keywords GitHub accepts in a pull-request body.
const CLOSING_KEYWORDS: [&str; 9] = [
    "close", "closes", "closed", "fix", "fixes", "fixed", "resolve", "resolves", "resolved",
];

/// Adds and removes closing-reference lines, returning `None` when the body would not change.
///
/// Only a line holding one closing keyword and one reference is a closing line. A line naming
/// several references, or any other mention of one, is left untouched, so a removal never rewrites
/// prose the caller wrote.
pub fn apply_closing_references(
    body: &str,
    add: &[ClosingReference],
    remove: &[ClosingReference],
) -> Option<String> {
    let mut changed = false;
    let mut kept: Vec<&str> = Vec::new();
    for line in body.split('\n') {
        match closing_line(line) {
            Some(reference) if remove.contains(&reference) => changed = true,
            _ => kept.push(line),
        }
    }
    let mut text = if changed {
        kept.join("\n").trim_end_matches('\n').to_owned()
    } else {
        body.to_owned()
    };
    for reference in add {
        if !closes(body, reference) {
            text = append(&text, &format!("Closes {reference}"));
            changed = true;
        }
    }
    changed.then_some(text)
}

/// The reference a line closes, when that line is exactly one closing keyword and one reference.
fn closing_line(line: &str) -> Option<ClosingReference> {
    let mut parts = line.split_whitespace();
    let keyword = parts.next()?;
    let reference = parts.next()?;
    if parts.next().is_some()
        || !CLOSING_KEYWORDS
            .iter()
            .any(|known| keyword.eq_ignore_ascii_case(known))
    {
        return None;
    }
    reference.parse::<ClosingReference>().ok()
}

/// True when `body` already carries a closing line for `reference`.
fn closes(body: &str, reference: &ClosingReference) -> bool {
    body.split('\n')
        .any(|line| closing_line(line).is_some_and(|found| found == *reference))
}

fn positive_number(value: &str) -> Option<u64> {
    value.parse::<u64>().ok().filter(|number| *number > 0)
}

#[cfg(test)]
mod tests {
    use super::{ClosingReference, append, apply_closing_references, replace_section};

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
            replace_section(&body, "Acceptance", "x").unwrap_err().code,
            "invalid_input"
        );
    }

    fn reference(value: &str) -> ClosingReference {
        value.parse().expect("valid closing reference")
    }

    #[test]
    fn adds_one_closing_line_per_reference_and_is_idempotent() {
        let add = [reference("#7"), reference("acme/service#12")];
        assert_eq!(
            apply_closing_references("Details.", &add, &[]).expect("changed"),
            "Details.\nCloses #7\nCloses acme/service#12"
        );
        assert_eq!(
            apply_closing_references("", &[reference("#7")], &[]).expect("changed"),
            "Closes #7"
        );
        assert!(apply_closing_references("Closes #7", &[reference("#7")], &[]).is_none());
        // A keyword-only mention of the issue is not a closing line, so the reference is added.
        assert_eq!(
            apply_closing_references("See #7 for context", &[reference("#7")], &[])
                .expect("changed"),
            "See #7 for context\nCloses #7"
        );
    }

    #[test]
    fn removes_only_the_named_closing_lines() {
        let body = "Details.\n\nCloses #5\nFixes #6\nSee #7 for context\nCloses #5, #6";
        assert_eq!(
            apply_closing_references(body, &[], &[reference("#5")]).expect("changed"),
            "Details.\n\nFixes #6\nSee #7 for context\nCloses #5, #6"
        );
        assert!(apply_closing_references(body, &[], &[reference("#9")]).is_none());
        // Keywords and repository names compare case-insensitively.
        assert_eq!(
            apply_closing_references(
                "intro\nCLOSES Acme/Service#12",
                &[],
                &[reference("acme/service#12")]
            )
            .expect("changed"),
            "intro"
        );
    }

    #[test]
    fn parses_and_formats_both_reference_forms() {
        assert_eq!(reference("#7").to_string(), "#7");
        assert_eq!(reference("7").number, 7);
        assert_eq!(reference("7").repo, None);
        assert_eq!(reference("acme/service#12").to_string(), "acme/service#12");
        for invalid in [
            "",
            "0",
            "#",
            "#0",
            "acme/service",
            "acme/service#0",
            "acme/service#x",
            "acme//service#1",
            "acme/#1",
            "acme/service/extra#1",
        ] {
            assert!(
                invalid.parse::<ClosingReference>().is_err(),
                "{invalid} must be rejected"
            );
        }
    }

    #[test]
    fn compares_repository_names_case_insensitively() {
        assert_eq!(reference("Acme/Service#7"), reference("acme/service#7"));
        assert_ne!(reference("#7"), reference("acme/service#7"));
        assert_ne!(reference("#7"), reference("#8"));
    }
}
