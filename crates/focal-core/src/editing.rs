//! Markdown-aware editing commands. Each returns the edit to apply rather than
//! changing the text, so the caller records it for undo.

use std::ops::Range;

/// An edit: replace `range` with `text`, then select `selection`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub range: Range<usize>,
    pub text: String,
    pub selection: Range<usize>,
}

struct ListPrefix {
    /// Indentation and quote markers before the list marker.
    lead: Range<usize>,
    marker: String,
    /// Where the item's content starts, after the marker and any task box.
    content: usize,
    task: bool,
}

fn list_prefix(text: &str, line: &Range<usize>) -> Option<ListPrefix> {
    let bytes = text.as_bytes();
    let mut pos = line.start;
    while pos < line.end && matches!(bytes[pos], b' ' | b'\t' | b'>') {
        pos += 1;
    }
    let lead = line.start..pos;
    let marker_start = pos;
    match bytes.get(pos) {
        Some(b'-' | b'*' | b'+') => pos += 1,
        Some(b'0'..=b'9') => {
            while pos < line.end && bytes[pos].is_ascii_digit() {
                pos += 1;
            }
            if !matches!(bytes.get(pos), Some(b'.' | b')')) {
                return None;
            }
            pos += 1;
        }
        _ => return None,
    }
    let marker = text[marker_start..pos].to_owned();
    if pos < line.end && bytes[pos] != b' ' {
        return None;
    }
    while pos < line.end && bytes[pos] == b' ' {
        pos += 1;
    }
    let rest = &text[pos..line.end];
    let task = ["[ ]", "[x]", "[X]"].iter().any(|b| rest.starts_with(b));
    if task {
        pos += 3;
        while pos < line.end && bytes[pos] == b' ' {
            pos += 1;
        }
    }
    Some(ListPrefix {
        lead,
        marker,
        content: pos,
        task,
    })
}

fn next_marker(marker: &str) -> String {
    let digits: String = marker.chars().take_while(char::is_ascii_digit).collect();
    match digits.parse::<u64>() {
        Ok(number) => format!("{}{}", number.saturating_add(1), &marker[digits.len()..]),
        Err(_) => marker.to_owned(),
    }
}

/// Return inside a list item continues the list; Return on an empty item ends
/// it. Returns `None` when an ordinary newline should be inserted.
pub fn continue_list(
    text: &str,
    line: &Range<usize>,
    selection: &Range<usize>,
    line_ending: &str,
) -> Option<Change> {
    let prefix = list_prefix(text, line)?;
    if selection.start < prefix.content {
        return None;
    }
    if text[prefix.content..line.end].trim().is_empty() {
        let start = prefix.lead.end;
        return Some(Change {
            range: start..line.end,
            text: String::new(),
            selection: start..start,
        });
    }
    let mut inserted = format!(
        "{line_ending}{}{} ",
        &text[prefix.lead.clone()],
        next_marker(&prefix.marker)
    );
    if prefix.task {
        inserted.push_str("[ ] ");
    }
    let caret = selection.start + inserted.len();
    Some(Change {
        range: selection.clone(),
        text: inserted,
        selection: caret..caret,
    })
}

/// Wraps the selection in `marker`, or unwraps it if it is already wrapped.
/// With an empty selection, inserts a pair and places the caret between.
pub fn toggle_wrap(text: &str, selection: &Range<usize>, marker: &str) -> Change {
    let len = marker.len();
    let before = selection.start.checked_sub(len).map(|s| s..selection.start);
    let after = selection.end..selection.end + len;
    let wrapped = before
        .as_ref()
        .is_some_and(|b| text.get(b.clone()) == Some(marker))
        && text.get(after.clone()) == Some(marker);
    if wrapped && let Some(before) = before {
        let inner = &text[selection.clone()];
        return Change {
            range: before.start..after.end,
            text: inner.to_owned(),
            selection: before.start..before.start + inner.len(),
        };
    }
    let inner = &text[selection.clone()];
    Change {
        range: selection.clone(),
        text: format!("{marker}{inner}{marker}"),
        selection: selection.start + len..selection.end + len,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn continues_bullets_numbers_and_tasks() {
        let text = "- item";
        let change = continue_list(text, &(0..6), &(6..6), "\n").unwrap();
        assert_eq!(change.text, "\n- ");
        let text = "  9. item";
        let change = continue_list(text, &(0..9), &(9..9), "\n").unwrap();
        assert_eq!(change.text, "\n  10. ");
        let text = "- [x] done";
        let change = continue_list(text, &(0..10), &(10..10), "\r\n").unwrap();
        assert_eq!(change.text, "\r\n- [ ] ");
    }

    #[test]
    fn return_on_an_empty_item_ends_the_list() {
        let text = "- one\n- ";
        let change = continue_list(text, &(6..8), &(8..8), "\n").unwrap();
        assert_eq!(change.range, 6..8);
        assert_eq!(change.text, "");
    }

    #[test]
    fn ignores_plain_lines_and_carets_before_the_marker() {
        assert!(continue_list("text", &(0..4), &(4..4), "\n").is_none());
        assert!(continue_list("-nope", &(0..5), &(5..5), "\n").is_none());
        assert!(continue_list("- item", &(0..6), &(0..0), "\n").is_none());
    }

    #[test]
    fn toggles_wrapping() {
        let change = toggle_wrap("a word", &(2..6), "**");
        assert_eq!(change.text, "**word**");
        assert_eq!(change.selection, 4..8);
        let change = toggle_wrap("a **word**", &(4..8), "**");
        assert_eq!(change.range, 2..10);
        assert_eq!(change.text, "word");
    }
}
