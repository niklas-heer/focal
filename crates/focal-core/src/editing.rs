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
    // A nested ordered list must start at 1: CommonMark only lets a list that
    // starts at 1 interrupt the paragraph of the item above.
    let digits = current
        .marker
        .bytes()
        .take_while(u8::is_ascii_digit)
        .count();
    let (replaced, number) = if digits > 0 && current.marker[..digits] != *"1" {
        (digits, "1")
    } else {
        (0, "")
    };
    let text = format!("{}{number}", " ".repeat(add));
    let caret = at + text.len();
    Some(Change {
        range: at..at + replaced,
        text,
        selection: caret..caret,
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

/// Maps an offset through edits applied inside a block of text. An offset
/// inside a replaced range moves to the end of its replacement.
fn map_offset(offset: usize, edits: &[(Range<usize>, String)]) -> usize {
    let mut mapped = offset;
    for (range, new) in edits {
        if range.end <= offset && range.start < offset {
            mapped = mapped + new.len() - range.len();
        } else if range.start < offset {
            mapped = mapped - (offset - range.start) + new.len();
        } else if range.start == offset && range.is_empty() {
            mapped += new.len();
        }
    }
    mapped
}

/// Applies non-overlapping `edits` (sorted, absolute ranges) inside `block` as
/// one change, carrying the selection along.
fn edit_block(
    text: &str,
    block: Range<usize>,
    edits: &[(Range<usize>, String)],
    selection: &Range<usize>,
) -> Change {
    let mut out = String::with_capacity(block.len() + 16);
    let mut at = block.start;
    for (range, new) in edits {
        out.push_str(&text[at..range.start]);
        out.push_str(new);
        at = range.end;
    }
    out.push_str(&text[at..block.end]);
    let block_start = block.start;
    let map = |offset: usize| {
        if offset < block_start {
            offset
        } else {
            map_offset(offset, edits)
        }
    };
    Change {
        range: block,
        text: out,
        selection: map(selection.start)..map(selection.end),
    }
}

/// The end of a line's quote markers (`> > `).
fn quote_end(text: &str, line: &Range<usize>) -> usize {
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
            return pos;
        }
    }
}

fn is_blank(text: &str, line: &Range<usize>) -> bool {
    text[quote_end(text, line)..line.end].trim().is_empty()
}

/// Makes every line the selection touches a heading of `level` (1 to 6), or a
/// paragraph with level 0, replacing any heading marker it has.
pub fn set_heading(text: &str, lines: &LineIndex, selection: &Range<usize>, level: u8) -> Change {
    let touched = lines.lines_of(selection);
    let block = lines.range(touched.start).start..lines.range(touched.end - 1).end;
    let marker = if level == 0 {
        String::new()
    } else {
        format!("{} ", "#".repeat(usize::from(level.min(6))))
    };
    let edits: Vec<_> = touched
        .map(|line| {
            let range = lines.range(line);
            let start = quote_end(text, &range);
            let rest = &text[start..range.end];
            let hashes = rest.bytes().take_while(|&b| b == b'#').count();
            let existing = if (1..=6).contains(&hashes)
                && rest[hashes..].chars().next().is_none_or(|c| c == ' ')
            {
                hashes + rest[hashes..].bytes().take_while(|&b| b == b' ').count()
            } else {
                0
            };
            (start..start + existing, marker.clone())
        })
        .collect();
    edit_block(text, block, &edits, selection)
}

/// A line prefix the formatting bar toggles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockPrefix {
    Bullet,
    Numbered,
    Task,
    Quote,
}

/// Adds `prefix` to every non-blank line the selection touches, or removes it
/// when they all have it already. Lists replace another kind of list marker.
pub fn toggle_prefix(
    text: &str,
    lines: &LineIndex,
    selection: &Range<usize>,
    prefix: BlockPrefix,
) -> Change {
    let touched = lines.lines_of(selection);
    let block = lines.range(touched.start).start..lines.range(touched.end - 1).end;
    let single = touched.len() == 1;
    let targets: Vec<Range<usize>> = touched
        .map(|line| lines.range(line))
        .filter(|range| single || !is_blank(text, range))
        .collect();
    let has = |range: &Range<usize>| {
        let list = list_prefix(text, range);
        match prefix {
            BlockPrefix::Bullet => {
                list.is_some_and(|l| !l.task && !l.marker.starts_with(|c: char| c.is_ascii_digit()))
            }
            BlockPrefix::Numbered => {
                list.is_some_and(|l| !l.task && l.marker.starts_with(|c: char| c.is_ascii_digit()))
            }
            BlockPrefix::Task => list.is_some_and(|l| l.task),
            BlockPrefix::Quote => quote_end(text, range) > range.start,
        }
    };
    let remove = !targets.is_empty() && targets.iter().all(has);
    let mut number = 0;
    let edits: Vec<_> = targets
        .iter()
        .filter_map(|range| {
            let list = list_prefix(text, range);
            if remove {
                return Some(match (prefix, list) {
                    (BlockPrefix::Quote, _) => {
                        let first = range.start + text[range.clone()].find('>')?;
                        let end = if text.as_bytes().get(first + 1) == Some(&b' ') {
                            first + 2
                        } else {
                            first + 1
                        };
                        (range.start..end, String::new())
                    }
                    (BlockPrefix::Task, Some(l)) => (l.marker_end..l.content, String::new()),
                    (_, Some(l)) => (l.lead.start..l.marker_end, String::new()),
                    (_, None) => return None,
                });
            }
            if has(range) {
                if prefix == BlockPrefix::Numbered {
                    number += 1;
                }
                return None;
            }
            let at = quote_end(text, range);
            Some(match (prefix, list) {
                (BlockPrefix::Quote, _) => (range.start..range.start, "> ".to_owned()),
                (BlockPrefix::Bullet, Some(l)) => (l.lead.end..l.content, "- ".to_owned()),
                (BlockPrefix::Bullet, None) => (at..at, "- ".to_owned()),
                (BlockPrefix::Numbered, list) => {
                    number += 1;
                    let marker = format!("{number}. ");
                    match list {
                        Some(l) => (l.lead.end..l.content, marker),
                        None => (at..at, marker),
                    }
                }
                (BlockPrefix::Task, Some(l)) => (l.marker_end..l.marker_end, "[ ] ".to_owned()),
                (BlockPrefix::Task, None) => (at..at, "- [ ] ".to_owned()),
            })
        })
        .collect();
    edit_block(text, block, &edits, selection)
}

/// Turns the selection into a link: `[text]()` with the caret in the
/// parentheses, `[](url)` for a selected URL, or `[]()` with the caret in the
/// brackets.
pub fn insert_link(text: &str, selection: &Range<usize>) -> Change {
    let inner = &text[selection.clone()];
    let start = selection.start;
    if inner.starts_with("http://") || inner.starts_with("https://") {
        return Change {
            range: selection.clone(),
            text: format!("[]({inner})"),
            selection: start + 1..start + 1,
        };
    }
    let caret = if inner.is_empty() {
        start + 1
    } else {
        start + inner.len() + 3
    };
    Change {
        range: selection.clone(),
        text: format!("[{inner}]()"),
        selection: caret..caret,
    }
}

/// A block the formatting bar inserts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Block {
    Table,
    CodeBlock,
    Math,
}

/// Inserts a table, code block or math block as its own paragraph. A code or
/// math block with lines selected fences those lines instead.
pub fn insert_block(
    text: &str,
    lines: &LineIndex,
    selection: &Range<usize>,
    block: Block,
) -> Change {
    let fence = match block {
        Block::Table => "",
        Block::CodeBlock => "```",
        Block::Math => "$$",
    };
    if !selection.is_empty() && block != Block::Table {
        let touched = lines.lines_of(selection);
        let range = lines.range(touched.start).start..lines.range(touched.end - 1).end;
        let edits = [
            (range.start..range.start, format!("{fence}\n")),
            (range.end..range.end, format!("\n{fence}")),
        ];
        let mut change = edit_block(text, range.clone(), &edits, selection);
        change.selection = selection.start + fence.len() + 1..selection.end + fence.len() + 1;
        return change;
    }
    let line = lines.line_of(selection.start);
    let range = lines.range(line);
    let blank = |line: usize| is_blank(text, &lines.range(line));
    let (at, lead) = if blank(line) {
        let lead = if line > 0 && !blank(line - 1) {
            "\n"
        } else {
            ""
        };
        (range.start, lead)
    } else {
        (range.end, "\n\n")
    };
    let trail = if line + 1 < lines.len() && !blank(line + 1) {
        "\n"
    } else {
        ""
    };
    let (body, caret) = match block {
        Block::Table => ("| Column | Column |\n| --- | --- |\n|  |  |".to_owned(), 2),
        _ => (format!("{fence}\n\n{fence}"), fence.len() + 1),
    };
    let caret = at + lead.len() + caret;
    Change {
        range: at..at,
        text: format!("{lead}{body}{trail}"),
        selection: caret..caret,
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
        // A nested ordered list must start at 1, or CommonMark reads the line
        // as a continuation of the item above.
        assert_eq!(apply(text, &change), "1. one\n   1. two");
        let text = "- a\n- b";
        let change = indent_list_item(text, &LineIndex::new(text), 1).unwrap();
        assert_eq!(apply(text, &change), "- a\n  - b");
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
    fn run(text: &str, change: impl Fn(&str, &LineIndex) -> Change) -> (String, Range<usize>) {
        let lines = LineIndex::new(text);
        let change = change(text, &lines);
        (apply(text, &change), change.selection)
    }

    #[test]
    fn headings_are_set_changed_and_removed() {
        let (out, sel) = run("a\nb", |t, l| set_heading(t, l, &(3..3), 2));
        assert_eq!(out, "a\n## b", "the last line without a newline");
        assert_eq!(sel, 6..6, "the caret stays after the same text");
        assert_eq!(run("## a", |t, l| set_heading(t, l, &(4..4), 3)).0, "### a");
        assert_eq!(run("## a", |t, l| set_heading(t, l, &(4..4), 0)).0, "a");
        assert_eq!(run("> a", |t, l| set_heading(t, l, &(3..3), 1)).0, "> # a");
        assert_eq!(
            run("#hashtag", |t, l| set_heading(t, l, &(0..0), 1)).0,
            "# #hashtag"
        );
        assert_eq!(run("", |t, l| set_heading(t, l, &(0..0), 1)).0, "# ");
    }

    #[test]
    fn bullets_toggle_on_and_off_across_lines() {
        let text = "one\n\ntwo\nthree";
        let (out, sel) = run(text, |t, l| {
            toggle_prefix(t, l, &(0..text.len()), BlockPrefix::Bullet)
        });
        assert_eq!(out, "- one\n\n- two\n- three", "blank lines stay blank");
        assert_eq!(sel, 2..out.len());
        let (back, _) = run(&out, |t, l| {
            toggle_prefix(t, l, &(0..t.len()), BlockPrefix::Bullet)
        });
        assert_eq!(back, text);
    }

    #[test]
    fn numbers_tasks_and_quotes() {
        assert_eq!(
            run("a\nb\nc", |t, l| toggle_prefix(
                t,
                l,
                &(0..5),
                BlockPrefix::Numbered
            ))
            .0,
            "1. a\n2. b\n3. c"
        );
        assert_eq!(
            run("- a\n- b", |t, l| toggle_prefix(
                t,
                l,
                &(0..7),
                BlockPrefix::Numbered
            ))
            .0,
            "1. a\n2. b",
            "another list's markers are replaced"
        );
        assert_eq!(
            run("- a", |t, l| toggle_prefix(
                t,
                l,
                &(3..3),
                BlockPrefix::Task
            ))
            .0,
            "- [ ] a"
        );
        assert_eq!(
            run("- [x] a", |t, l| toggle_prefix(
                t,
                l,
                &(7..7),
                BlockPrefix::Task
            ))
            .0,
            "- a"
        );
        assert_eq!(
            run("a", |t, l| toggle_prefix(t, l, &(1..1), BlockPrefix::Task)).0,
            "- [ ] a"
        );
        assert_eq!(
            run("", |t, l| toggle_prefix(t, l, &(0..0), BlockPrefix::Quote)).0,
            "> "
        );
        assert_eq!(
            run("> a", |t, l| toggle_prefix(
                t,
                l,
                &(3..3),
                BlockPrefix::Quote
            ))
            .0,
            "a"
        );
        assert_eq!(
            run("- a\nplain", |t, l| toggle_prefix(
                t,
                l,
                &(0..9),
                BlockPrefix::Bullet
            ))
            .0,
            "- a\n- plain",
            "a selection spanning a list and text becomes one list"
        );
    }

    #[test]
    fn links_wrap_the_selection() {
        let (out, sel) = run("see here", |t, _| insert_link(t, &(4..8)));
        assert_eq!((out.as_str(), sel), ("see [here]()", 11..11));
        let (out, sel) = run("x", |t, _| insert_link(t, &(1..1)));
        assert_eq!((out.as_str(), sel), ("x[]()", 2..2));
        let (out, sel) = run("https://a.b", |t, _| insert_link(t, &(0..11)));
        assert_eq!((out.as_str(), sel), ("[](https://a.b)", 1..1));
    }

    #[test]
    fn blocks_start_their_own_paragraph() {
        let (out, sel) = run("text\nmore", |t, l| {
            insert_block(t, l, &(2..2), Block::Table)
        });
        assert_eq!(
            out,
            "text\n\n| Column | Column |\n| --- | --- |\n|  |  |\n\nmore"
        );
        assert_eq!(sel, 8..8, "the caret is in the first header cell");
        let (out, sel) = run("", |t, l| insert_block(t, l, &(0..0), Block::CodeBlock));
        assert_eq!((out.as_str(), sel), ("```\n\n```", 4..4));
        let (out, _) = run("a\n\n", |t, l| insert_block(t, l, &(2..2), Block::Math));
        assert_eq!(
            out, "a\n\n$$\n\n$$\n",
            "the blank line separates, the last newline stays"
        );
        let (out, sel) = run("x\ny", |t, l| insert_block(t, l, &(0..3), Block::CodeBlock));
        assert_eq!((out.as_str(), sel), ("```\nx\ny\n```", 4..7));
    }
}
