use crate::domain::AppError;

/// Applies a unified diff to `body` with exact context matching.
///
/// Each hunk's pre-image (context and removed lines) must appear verbatim in the body at or after
/// the position where the previous hunk ended. Location is content-based, so line numbers in the
/// patch are validated but not trusted. There is no fuzzy matching: shifted or edited context is a
/// hard failure rather than a silent misapplication.
pub fn apply(body: &str, patch: &str) -> Result<String, AppError> {
    let hunks = parse(patch)?;
    let mut lines: Vec<String> = body.split('\n').map(str::to_owned).collect();
    let mut cursor = 0;
    for hunk in &hunks {
        let start = lines[cursor..]
            .windows(hunk.pre.len())
            .position(|window| window == hunk.pre.as_slice())
            .map(|offset| cursor + offset)
            .ok_or(AppError::patch_conflict())?;
        let inserted = hunk.post.len();
        lines.splice(start..start + hunk.pre.len(), hunk.post.clone());
        cursor = start + inserted;
    }
    Ok(lines.join("\n"))
}

struct Hunk {
    pre: Vec<String>,
    post: Vec<String>,
}

/// Preamble lines before the first `@@` header (file names, `diff --git`, index) are ignored.
fn parse(patch: &str) -> Result<Vec<Hunk>, AppError> {
    let mut hunks: Vec<Hunk> = Vec::new();
    let mut current: Option<Hunk> = None;
    // The final newline is a terminator, not an empty context line.
    let text = patch.strip_suffix('\n').unwrap_or(patch);
    for line in text.split('\n') {
        if line.starts_with("@@") {
            validate_header(line)?;
            if let Some(hunk) = current.take() {
                push(hunk, &mut hunks)?;
            }
            current = Some(Hunk {
                pre: Vec::new(),
                post: Vec::new(),
            });
            continue;
        }
        let Some(hunk) = current.as_mut() else {
            continue;
        };
        if line.starts_with("\\ ") {
            continue;
        }
        match line.chars().next() {
            Some(' ') => {
                let content = line[1..].to_owned();
                hunk.pre.push(content.clone());
                hunk.post.push(content);
            }
            Some('-') => hunk.pre.push(line[1..].to_owned()),
            Some('+') => hunk.post.push(line[1..].to_owned()),
            // A bare empty line is a context line with empty content in most diff output.
            None => {
                hunk.pre.push(String::new());
                hunk.post.push(String::new());
            }
            Some(_) => {
                return Err(AppError::invalid_input(
                    "patch contains a line that is not part of a hunk",
                ));
            }
        }
    }
    if let Some(hunk) = current.take() {
        push(hunk, &mut hunks)?;
    }
    if hunks.is_empty() {
        return Err(AppError::invalid_input("patch contains no hunks"));
    }
    Ok(hunks)
}

/// A hunk with no pre-image cannot be located; a pure insertion needs at least one context line.
fn push(hunk: Hunk, hunks: &mut Vec<Hunk>) -> Result<(), AppError> {
    if hunk.pre.is_empty() {
        return Err(AppError::invalid_input(
            "each patch hunk needs at least one context or removed line",
        ));
    }
    hunks.push(hunk);
    Ok(())
}

fn validate_header(line: &str) -> Result<(), AppError> {
    let malformed = || AppError::invalid_input("patch hunk header is malformed");
    let rest = line.strip_prefix("@@ -").ok_or_else(malformed)?;
    let (old, rest) = rest.split_once(" +").ok_or_else(malformed)?;
    let (new, _) = rest.split_once(" @@").ok_or_else(malformed)?;
    parse_range(old)?;
    parse_range(new)
}

fn parse_range(text: &str) -> Result<(), AppError> {
    let malformed = || AppError::invalid_input("patch hunk header is malformed");
    let (start, count) = match text.split_once(',') {
        Some((start, count)) => (start, count),
        None => (text, "1"),
    };
    start.parse::<u32>().map_err(|_| malformed())?;
    count.parse::<u32>().map_err(|_| malformed())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::apply;

    const BODY: &str = "# Title\n\nintro paragraph\n\n## Acceptance\n\n- first\n- second\n- third\n\n## Notes\n\nkeep me\n";

    #[test]
    fn applies_a_single_hunk_in_the_middle() {
        let patch = "--- a/issue\n+++ b/issue\n@@ -7,4 +7,4 @@\n \n - first\n-- second\n+- second (revised)\n - third\n";
        let updated = apply(BODY, patch).expect("apply patch");
        assert_eq!(
            updated,
            "# Title\n\nintro paragraph\n\n## Acceptance\n\n- first\n- second (revised)\n- third\n\n## Notes\n\nkeep me\n"
        );
        assert!(updated.contains("keep me"));
    }

    #[test]
    fn applies_multiple_hunks_in_order() {
        let patch = "@@ -3,1 +3,1 @@\n-intro paragraph\n+intro rewritten\n@@ -12,1 +12,1 @@\n-keep me\n+also rewritten\n";
        let updated = apply(BODY, patch).expect("apply patch");
        assert_eq!(
            updated,
            "# Title\n\nintro rewritten\n\n## Acceptance\n\n- first\n- second\n- third\n\n## Notes\n\nalso rewritten\n"
        );
    }

    #[test]
    fn inserts_and_deletes_anchored_lines() {
        let patch = "@@ -8,2 +8,3 @@\n - first\n+- inserted\n - second\n";
        let updated = apply(BODY, patch).expect("insert");
        assert!(updated.contains("- first\n- inserted\n- second"));

        let deletion = "@@ -8,3 +8,2 @@\n - first\n-- second\n - third\n";
        let updated = apply(BODY, deletion).expect("delete");
        assert!(updated.contains("- first\n- third"));
    }

    #[test]
    fn tolerates_preamble_and_no_newline_markers() {
        let patch = "diff --git a/issue b/issue\nindex 111..222 100644\n--- a/issue\n+++ b/issue\n@@ -12,1 +12,1 @@\n-keep me\n\\ No newline at end of file\n+keep me too\n";
        let updated = apply(BODY, patch).expect("apply patch");
        assert!(updated.contains("keep me too"));
    }

    #[test]
    fn rejects_context_that_does_not_match() {
        let patch = "@@ -3,1 +3,1 @@\n-a different paragraph\n+replacement\n";
        assert_eq!(apply(BODY, patch).unwrap_err().code, "patch_conflict");
    }

    #[test]
    fn rejects_hunks_without_context() {
        let patch = "@@ -0,0 +1,1 @@\n+prepended\n";
        assert_eq!(apply(BODY, patch).unwrap_err().code, "invalid_input");
    }

    #[test]
    fn rejects_malformed_or_empty_patches() {
        assert_eq!(
            apply(BODY, "@@ -x,1 +1,1 @@\n-a\n+b\n").unwrap_err().code,
            "invalid_input"
        );
        assert_eq!(
            apply(BODY, "--- a/issue\n+++ b/issue\n").unwrap_err().code,
            "invalid_input"
        );
        assert_eq!(
            apply(BODY, "@@ -3,1 +3,1 @@\n?nonsense\n")
                .unwrap_err()
                .code,
            "invalid_input"
        );
    }

    #[test]
    fn applies_the_last_hunk_when_the_body_has_no_trailing_newline() {
        let patch = "@@ -3,1 +3,1 @@\n-intro paragraph\n+intro rewritten\n";
        let updated = apply("first\nintro paragraph\nlast", patch).expect("apply patch");
        assert_eq!(updated, "first\nintro rewritten\nlast");
    }
}
