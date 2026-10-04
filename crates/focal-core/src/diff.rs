//! What changed between a baseline and the current text, word by word, for
//! drawing an agent's rewrite: inserted ranges in the current text, deleted
//! text at its place, and the changed regions to step through.

use std::ops::Range;
use std::time::Instant;

use similar::{ChangeTag, DiffTag, TextDiff};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Changes {
    /// Inserted or changed text, as ranges of the current text.
    pub inserted: Vec<Range<usize>>,
    /// Deleted text, at the offset in the current text where it was.
    pub deleted: Vec<(usize, String)>,
    /// Each changed region of the current text (empty where only text was
    /// deleted), in order.
    pub hunks: Vec<Range<usize>>,
}

impl Changes {
    /// How many changed regions there are.
    pub fn count(&self) -> usize {
        self.hunks.len()
    }
}

/// The changes from `old` to `new`: lines first, then the words within
/// changed lines. Offsets stay on character boundaries.
pub fn changes(old: &str, new: &str) -> Changes {
    changes_with(old, new, None)
}

/// [`changes`], giving up on precision at `deadline`: past it, changed
/// lines are marked whole.
pub fn changes_until(old: &str, new: &str, deadline: Instant) -> Changes {
    changes_with(old, new, Some(deadline))
}

fn changes_with(old: &str, new: &str, deadline: Option<Instant>) -> Changes {
    let mut out = Changes::default();
    let mut config = TextDiff::configure();
    if let Some(deadline) = deadline {
        config.deadline(deadline);
    }
    let lines = config.diff_lines(old, new);
    let old_starts = starts(lines.old_slices());
    let new_starts = starts(lines.new_slices());
    let mut hunk: Option<Range<usize>> = None;
    for op in lines.ops() {
        let (old_lines, new_lines) = (op.old_range(), op.new_range());
        let old_bytes = old_starts[old_lines.start]..old_starts[old_lines.end];
        let new_bytes = new_starts[new_lines.start]..new_starts[new_lines.end];
        match op.tag() {
            DiffTag::Equal => {
                out.hunks.extend(hunk.take());
                continue;
            }
            DiffTag::Insert => out.inserted.push(new_bytes.clone()),
            DiffTag::Delete => out
                .deleted
                .push((new_bytes.start, old[old_bytes].to_owned())),
            DiffTag::Replace => {
                if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                    out.inserted.push(new_bytes.clone());
                } else {
                    words(
                        &old[old_bytes],
                        &new[new_bytes.clone()],
                        new_bytes.start,
                        &mut out,
                    );
                }
            }
        }
        hunk = Some(match hunk {
            Some(hunk) => hunk.start..new_bytes.end,
            None => new_bytes,
        });
    }
    out.hunks.extend(hunk);
    tidy(&mut out.inserted, new);
    out
}

/// Where each slice starts, and the end after the last.
fn starts(slices: &[&str]) -> Vec<usize> {
    let mut at = 0;
    let mut starts = Vec::with_capacity(slices.len() + 1);
    starts.push(0);
    for slice in slices {
        at += slice.len();
        starts.push(at);
    }
    starts
}

/// The word-level changes inside replaced lines, `base` being where `new`
/// starts in the whole text.
fn words(old: &str, new: &str, base: usize, out: &mut Changes) {
    let diff = TextDiff::configure().diff_unicode_words(old, new);
    let mut at = base;
    for change in diff.iter_all_changes() {
        let value = change.value();
        match change.tag() {
            ChangeTag::Equal => at += value.len(),
            ChangeTag::Insert => {
                out.inserted.push(at..at + value.len());
                at += value.len();
            }
            ChangeTag::Delete => {
                if !value.trim().is_empty() {
                    match out.deleted.last_mut() {
                        // Words deleted together read as one deletion.
                        Some((offset, text)) if *offset == at => text.push_str(value),
                        _ => out.deleted.push((at, value.to_owned())),
                    }
                }
            }
        }
    }
}

/// Joins touching ranges, so "quick " is one tint rather than two, and
/// drops what is only whitespace.
fn tidy(ranges: &mut Vec<Range<usize>>, text: &str) {
    ranges.sort_by_key(|range| range.start);
    let mut merged: Vec<Range<usize>> = Vec::with_capacity(ranges.len());
    for range in ranges.drain(..) {
        match merged.last_mut() {
            Some(last) if last.end >= range.start => last.end = last.end.max(range.end),
            _ => merged.push(range),
        }
    }
    merged.retain(|range| !text[range.clone()].trim().is_empty());
    *ranges = merged;
}

/// The hunk after `from` going forward (one starting after it), or the
/// one before it going backward, wrapping around.
pub fn next_hunk(hunks: &[Range<usize>], from: usize, forward: bool) -> Option<usize> {
    if hunks.is_empty() {
        return None;
    }
    Some(if forward {
        hunks.iter().position(|hunk| hunk.start > from).unwrap_or(0)
    } else {
        hunks
            .iter()
            .rposition(|hunk| hunk.start < from)
            .unwrap_or(hunks.len() - 1)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inserted_words_and_deleted_text() {
        let new = "The quick fox leaps.\nSame line.\n";
        let c = changes("The fox jumps.\nSame line.\n", new);
        let inserted: Vec<&str> = c.inserted.iter().map(|r| &new[r.clone()]).collect();
        assert_eq!(inserted, ["quick ", "leaps"]);
        assert_eq!(c.deleted, [(14, "jumps".to_owned())]);
        assert_eq!(c.count(), 1, "one changed line is one hunk");
    }

    #[test]
    fn deleted_lines_and_added_lines() {
        let c = changes("a\nb\nc\n", "a\nc\nd\n");
        assert_eq!(c.deleted, [(2, "b\n".to_owned())]);
        assert_eq!(c.inserted, std::iter::once(4..6).collect::<Vec<_>>());
        assert_eq!(c.count(), 2);
    }

    #[test]
    fn umlauts_and_crlf() {
        let new = "Grüße, Welt\r\nzwei\r\n";
        let c = changes("Grüße\r\nzwei\r\n", new);
        for r in &c.inserted {
            assert!(new.is_char_boundary(r.start) && new.is_char_boundary(r.end));
        }
        let inserted: Vec<&str> = c.inserted.iter().map(|r| &new[r.clone()]).collect();
        assert_eq!(inserted, [", Welt"]);
        assert_eq!(c.count(), 1);
    }

    #[test]
    fn a_deadline_only_coarsens_the_result() {
        let (old, new) = ("a b c\nsame\n", "a x c\nsame\n");
        let later = std::time::Instant::now() + std::time::Duration::from_secs(5);
        assert_eq!(changes_until(old, new, later), changes(old, new));
        let past = std::time::Instant::now();
        let coarse = changes_until(old, new, past);
        assert_eq!(coarse.count(), 1, "still one change");
        assert!(
            coarse
                .inserted
                .iter()
                .all(|r| new.is_char_boundary(r.start) && new.is_char_boundary(r.end))
        );
    }

    #[test]
    fn identical_texts_have_no_changes() {
        assert_eq!(changes("x\n", "x\n"), Changes::default());
    }

    #[test]
    fn a_new_file_is_all_inserted() {
        let c = changes("", "one\ntwo\n");
        assert_eq!(c.inserted, std::iter::once(0..8).collect::<Vec<_>>());
        assert_eq!(c.count(), 1);
    }

    #[test]
    fn hunks_are_found_in_order_and_wrap() {
        let hunks = [2..4, 10..12];
        assert_eq!(next_hunk(&hunks, 0, true), Some(0));
        assert_eq!(
            next_hunk(&hunks, 2, true),
            Some(1),
            "from inside one, the next"
        );
        assert_eq!(next_hunk(&hunks, 11, true), Some(0), "wraps");
        assert_eq!(next_hunk(&hunks, 10, false), Some(0));
        assert_eq!(next_hunk(&hunks, 2, false), Some(1), "wraps back");
        assert_eq!(next_hunk(&[], 0, true), None);
    }
}
