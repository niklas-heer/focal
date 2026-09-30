//! Markdown-aware editing commands. Each returns the edit to apply rather than
//! changing the text, so the caller records it for undo.

use std::ops::Range;

use crate::analysis::Analysis;
use crate::lines::LineIndex;

/// An edit: replace `range` with `text`, then select `selection`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub range: Range<usize>,
    pub text: String,
    pub selection: Range<usize>,
}

struct ListPrefix {
    /// Quote markers (with their spaces) before the list's indentation.
    quotes: Range<usize>,
    /// Indentation between the quote markers and the list marker.
    lead: Range<usize>,
    marker: String,
    /// After the marker and its spaces, before any task box.
    marker_end: usize,
    /// Where the item's content starts, after the marker and any task box.
    content: usize,
    task: bool,
}

fn list_prefix(text: &str, line: &Range<usize>) -> Option<ListPrefix> {
    let bytes = text.as_bytes();
    let mut pos = line.start;
    loop {
        let mut at = pos;
        while at < line.end && bytes[at] == b' ' {
            at += 1;
        }
        if at < line.end && bytes[at] == b'>' {
            pos = at + 1;
            if pos < line.end && bytes[pos] == b' ' {
                pos += 1;
            }
        } else {
            break;
        }
    }
    let quotes = line.start..pos;
    while pos < line.end && matches!(bytes[pos], b' ' | b'\t') {
        pos += 1;
    }
    let lead = quotes.end..pos;
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
    let marker_end = pos;
    let rest = &text[pos..line.end];
    let task = ["[ ]", "[x]", "[X]"].iter().any(|b| rest.starts_with(b));
    if task {
        pos += 3;
        while pos < line.end && bytes[pos] == b' ' {
            pos += 1;
        }
    }
    Some(ListPrefix {
        quotes,
        lead,
        marker,
        marker_end,
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
        // An empty nested item moves out a level; an empty top-level item ends the list.
        if !prefix.lead.is_empty() {
            let lines = LineIndex::new(text);
            return outdent_list_item(text, &lines, lines.line_of(line.start));
        }
        let start = prefix.lead.end;
        return Some(Change {
            range: start..line.end,
            text: String::new(),
            selection: start..start,
        });
    }
    let mut inserted = format!(
        "{line_ending}{}{} ",
        &text[prefix.quotes.start..prefix.lead.end],
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

/// Backspace at a line's content start edits the prefix instead of text: it
/// removes the list marker, else the innermost quote marker, else joins the
/// line to the previous one. `None` when `head` is not at such a place.
pub fn backspace_prefix(analysis: &Analysis, head: usize) -> Option<Change> {
    let line = analysis.lines.line_of(head);
    let content = analysis.content_range(line);
    let line_start = analysis.lines.range(line).start;
    if head != content.start || content.start == line_start {
        return None;
    }
    let prefix = &analysis.info(line).prefix;
    let range = if let Some(marker) = &prefix.marker {
        marker.clone()
    } else if let Some(quote) = prefix.quotes.last() {
        quote.clone()
    } else if line > 0 {
        analysis.lines.range(line - 1).end..content.start
    } else {
        line_start..content.start
    };
    Some(Change {
        selection: range.start..range.start,
        range,
        text: String::new(),
    })
}

/// Earlier lines that are list items, nearest first.
fn previous_items<'t>(
    text: &'t str,
    lines: &'t LineIndex,
    line: usize,
) -> impl Iterator<Item = ListPrefix> + 't {
    (0..line)
        .rev()
        .filter_map(move |l| list_prefix(text, &lines.range(l)))
}

/// Tab on a list item: indent it to its previous sibling's content column.
pub fn indent_list_item(text: &str, lines: &LineIndex, line: usize) -> Option<Change> {
    let current = list_prefix(text, &lines.range(line))?;
    let sibling = previous_items(text, lines, line).find(|p| p.lead.len() == current.lead.len())?;
    let column = sibling.marker_end - sibling.lead.start;
    let add = column.saturating_sub(current.lead.len());
    if add == 0 {
        return None;
    }
    let at = current.lead.end;
    Some(Change {
        range: at..at,
        text: " ".repeat(add),
        selection: at + add..at + add,
    })
}

/// Shift-Tab on a list item: outdent it to its parent item's indentation.
pub fn outdent_list_item(text: &str, lines: &LineIndex, line: usize) -> Option<Change> {
    let current = list_prefix(text, &lines.range(line))?;
    if current.lead.is_empty() {
        return None;
    }
    let parent = previous_items(text, lines, line)
        .find(|p| p.lead.len() < current.lead.len())
        .map_or(0, |p| p.lead.len());
    let remove = current.lead.len() - parent;
    let end = current.lead.end;
    Some(Change {
        range: end - remove..end,
        text: String::new(),
        selection: end - remove..end - remove,
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
    use crate::analysis::analyze;
    use crate::lines::LineIndex;

    fn apply(text: &str, change: &Change) -> String {
        let mut out = text.to_owned();
        out.replace_range(change.range.clone(), &change.text);
        out
    }

    #[test]
    fn backspace_removes_the_marker_then_quotes_then_joins() {
        let text = "- [ ] task\n> quote\nnext\n  cont";
        let analysis = analyze(text);
        let change = backspace_prefix(&analysis, 6).unwrap();
        assert_eq!(apply(text, &change), "task\n> quote\nnext\n  cont");
        let change = backspace_prefix(&analysis, 13).unwrap();
        assert_eq!(apply(text, &change), "- [ ] task\nquote\nnext\n  cont");
        assert!(
            backspace_prefix(&analysis, 19).is_none(),
            "no prefix on `next`"
        );
        assert!(
            backspace_prefix(&analysis, 7).is_none(),
            "not at the content start"
        );
    }

    #[test]
    fn backspace_on_line_zero_never_leaves_the_prefix() {
        let text = "> hi";
        let analysis = analyze(text);
        assert_eq!(apply(text, &backspace_prefix(&analysis, 2).unwrap()), "hi");
        assert!(backspace_prefix(&analyze(""), 0).is_none());
    }

    #[test]
    fn tab_indents_to_the_previous_siblings_content() {
        let text = "1. one\n2. two";
        let lines = LineIndex::new(text);
        let change = indent_list_item(text, &lines, 1).unwrap();
        assert_eq!(apply(text, &change), "1. one\n   2. two");
        assert!(
            indent_list_item(text, &lines, 0).is_none(),
            "first item has no sibling"
        );
    }

    #[test]
    fn shift_tab_outdents_to_the_parent() {
        let text = "- a\n  - b";
        let lines = LineIndex::new(text);
        assert_eq!(
            apply(text, &outdent_list_item(text, &lines, 1).unwrap()),
            "- a\n- b"
        );
        assert!(outdent_list_item(text, &lines, 0).is_none());
    }

    #[test]
    fn return_on_an_empty_nested_item_outdents_it() {
        let text = "- a\n  - ";
        let change = continue_list(text, &(4..8), &(8..8), "\n").unwrap();
        assert_eq!(apply(text, &change), "- a\n- ");
    }

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
