//! The document text, its edits and their undo history.

use std::ops::Range;
use std::time::{Duration, Instant};

use unicode_segmentation::UnicodeSegmentation;

/// Consecutive typing within this interval undoes as one step.
const COALESCE: Duration = Duration::from_millis(1200);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditKind {
    Typing,
    Deleting,
    Other,
    /// An edit of a table cell; all edits in one session undo together.
    Grid(u64),
}

#[derive(Clone, Debug)]
struct Transaction {
    start: usize,
    old: String,
    new: String,
    selection_before: Range<usize>,
    selection_after: Range<usize>,
    kind: EditKind,
    at: Instant,
}

/// The text of a document. The bytes on disk are the bytes held here; Focal
/// never normalizes them.
#[derive(Clone, Debug, Default)]
pub struct Buffer {
    text: String,
    undo: Vec<Transaction>,
    redo: Vec<Transaction>,
    version: u64,
}

impl Buffer {
    pub fn new(text: String) -> Self {
        Self {
            text,
            ..Self::default()
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// Increases with every change, including undo and redo.
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// The line ending new lines should use: CRLF if the file already uses it.
    pub fn line_ending(&self) -> &'static str {
        if self.text.contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        }
    }

    /// Replaces `range` with `new` and records the change for undo.
    pub fn edit(
        &mut self,
        range: Range<usize>,
        new: &str,
        selection_before: Range<usize>,
        selection_after: Range<usize>,
        kind: EditKind,
    ) {
        let old = self.text[range.clone()].to_owned();
        self.text.replace_range(range.clone(), new);
        self.version += 1;
        self.redo.clear();
        let now = Instant::now();
        if let EditKind::Grid(session) = kind
            && let Some(last) = self.undo.last_mut()
            && last.kind == EditKind::Grid(session)
            && range == (last.start..last.start + last.new.len())
        {
            new.clone_into(&mut last.new);
            last.selection_after = selection_after;
            last.at = now;
            return;
        }
        if let Some(last) = self.undo.last_mut()
            && last.kind == kind
            && now.duration_since(last.at) < COALESCE
        {
            let typed_on = kind == EditKind::Typing
                && old.is_empty()
                && last.old.is_empty()
                && last.start + last.new.len() == range.start
                && !new.contains('\n');
            let deleted_back = kind == EditKind::Deleting
                && new.is_empty()
                && last.new.is_empty()
                && range.end == last.start;
            if typed_on {
                last.new.push_str(new);
                last.selection_after = selection_after;
                last.at = now;
                return;
            }
            if deleted_back {
                last.start = range.start;
                last.old.insert_str(0, &old);
                last.selection_after = selection_after;
                last.at = now;
                return;
            }
        }
        self.undo.push(Transaction {
            start: range.start,
            old,
            new: new.to_owned(),
            selection_before,
            selection_after,
            kind,
            at: now,
        });
    }

    /// Reverts the last change and returns the selection to restore.
    pub fn undo(&mut self) -> Option<Range<usize>> {
        let transaction = self.undo.pop()?;
        let range = transaction.start..transaction.start + transaction.new.len();
        self.text.replace_range(range, &transaction.old);
        self.version += 1;
        let selection = transaction.selection_before.clone();
        self.redo.push(transaction);
        Some(selection)
    }

    pub fn redo(&mut self) -> Option<Range<usize>> {
        let mut transaction = self.redo.pop()?;
        let range = transaction.start..transaction.start + transaction.old.len();
        self.text.replace_range(range, &transaction.new);
        self.version += 1;
        let selection = transaction.selection_after.clone();
        // A redone change never merges with the next one.
        transaction.kind = EditKind::Other;
        self.undo.push(transaction);
        Some(selection)
    }

    /// Replaces the whole text, for example after the file changed on disk.
    /// The change is undoable like any other.
    pub fn replace_all(&mut self, text: &str, selection: Range<usize>) {
        let range = 0..self.text.len();
        self.edit(range, text, selection.clone(), selection, EditKind::Other);
    }

    pub fn previous_grapheme(&self, offset: usize) -> usize {
        self.text[..offset]
            .grapheme_indices(true)
            .next_back()
            .map_or(0, |(ix, _)| ix)
    }

    pub fn next_grapheme(&self, offset: usize) -> usize {
        self.text[offset..]
            .graphemes(true)
            .next()
            .map_or(offset, |g| offset + g.len())
    }

    pub fn previous_word(&self, offset: usize) -> usize {
        self.text[..offset]
            .split_word_bound_indices()
            .rev()
            .find(|(_, word)| word.chars().any(char::is_alphanumeric))
            .map_or(0, |(ix, _)| ix)
    }

    pub fn next_word(&self, offset: usize) -> usize {
        self.text[offset..]
            .split_word_bound_indices()
            .find(|(_, word)| word.chars().any(char::is_alphanumeric))
            .map_or(self.text.len(), |(ix, word)| offset + ix + word.len())
    }

    /// The word around `offset`, for double-click selection. Underscores at
    /// the edges are emphasis markers, not part of the word.
    pub fn word_at(&self, offset: usize) -> Range<usize> {
        for (ix, word) in self.text.split_word_bound_indices() {
            let range = ix..ix + word.len();
            if range.contains(&offset) || range.end == offset && offset == self.text.len() {
                let trimmed = word.trim_matches('_');
                if trimmed.is_empty() {
                    return range;
                }
                let start = ix + (word.len() - word.trim_start_matches('_').len());
                return start..start + trimmed.len();
            }
        }
        offset..offset
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_edits_in_one_session_undo_together() {
        let mut buffer = Buffer::new("x | a | y".into());
        buffer.edit(4..5, "ab", 0..0, 0..0, EditKind::Grid(1));
        buffer.edit(4..6, "abc", 0..0, 0..0, EditKind::Grid(1));
        buffer.edit(4..7, "abcd", 0..0, 0..0, EditKind::Grid(2));
        assert_eq!(buffer.text(), "x | abcd | y");
        buffer.undo();
        assert_eq!(
            buffer.text(),
            "x | abc | y",
            "a new session is its own step"
        );
        buffer.undo();
        assert_eq!(buffer.text(), "x | a | y", "one session is one step");
    }

    #[test]
    fn typing_coalesces_into_one_undo() {
        let mut buffer = Buffer::new(String::new());
        for (ix, ch) in "abc".chars().enumerate() {
            buffer.edit(
                ix..ix,
                &ch.to_string(),
                ix..ix,
                ix + 1..ix + 1,
                EditKind::Typing,
            );
        }
        assert_eq!(buffer.text(), "abc");
        assert_eq!(buffer.undo(), Some(0..0));
        assert_eq!(buffer.text(), "");
        assert_eq!(buffer.redo(), Some(3..3));
        assert_eq!(buffer.text(), "abc");
    }

    #[test]
    fn backspaces_coalesce() {
        let mut buffer = Buffer::new("abc".into());
        buffer.edit(2..3, "", 3..3, 2..2, EditKind::Deleting);
        buffer.edit(1..2, "", 2..2, 1..1, EditKind::Deleting);
        assert_eq!(buffer.text(), "a");
        buffer.undo();
        assert_eq!(buffer.text(), "abc");
    }

    #[test]
    fn newlines_start_a_new_undo_step() {
        let mut buffer = Buffer::new(String::new());
        buffer.edit(0..0, "a", 0..0, 1..1, EditKind::Typing);
        buffer.edit(1..1, "\n", 1..1, 2..2, EditKind::Typing);
        buffer.undo();
        assert_eq!(buffer.text(), "a");
    }

    #[test]
    fn word_selection_skips_emphasis_underscores() {
        let buffer = Buffer::new("an _emphasis_ and snake_case".into());
        assert_eq!(&buffer.text()[buffer.word_at(6)], "emphasis");
        assert_eq!(&buffer.text()[buffer.word_at(20)], "snake_case");
    }

    #[test]
    fn detects_crlf() {
        assert_eq!(Buffer::new("a\r\nb".into()).line_ending(), "\r\n");
        assert_eq!(Buffer::new("a\nb".into()).line_ending(), "\n");
    }

    #[test]
    fn moves_by_grapheme_and_word() {
        let buffer = Buffer::new("héllo wörld".into());
        assert_eq!(buffer.next_grapheme(1), 3);
        assert_eq!(buffer.previous_grapheme(3), 1);
        assert_eq!(buffer.next_word(0), 6);
        assert_eq!(buffer.previous_word(buffer.text().len()), 7);
    }
}
