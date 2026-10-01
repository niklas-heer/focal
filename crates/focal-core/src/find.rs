//! Find and replace: case-insensitive matches of a query, as byte ranges.

use std::ops::Range;

use crate::editing::Change;

/// The non-overlapping matches of `query` in `text`, in order, ignoring case.
/// Characters compare by their lowercase forms one by one, so a match always
/// covers whole characters (no full case folding: `ß` is not `SS`).
pub fn find_all(text: &str, query: &str) -> Vec<Range<usize>> {
    let mut matches = Vec::new();
    if query.is_empty() {
        return matches;
    }
    let mut start = 0;
    while start < text.len() {
        match match_at(text, start, query) {
            Some(end) => {
                matches.push(start..end);
                start = end;
            }
            None => start += text[start..].chars().next().map_or(1, char::len_utf8),
        }
    }
    matches
}

/// Where a match of `query` starting at `start` ends.
fn match_at(text: &str, start: usize, query: &str) -> Option<usize> {
    let mut chars = text[start..].char_indices();
    for wanted in query.chars() {
        let (_, found) = chars.next()?;
        if found != wanted && !found.to_lowercase().eq(wanted.to_lowercase()) {
            return None;
        }
    }
    Some(
        chars
            .next()
            .map_or(text.len(), |(offset, _)| start + offset),
    )
}

/// The first match starting at or after `from`, wrapping to the first.
pub fn next_match(matches: &[Range<usize>], from: usize) -> Option<usize> {
    if matches.is_empty() {
        return None;
    }
    let ix = matches.partition_point(|m| m.start < from);
    Some(if ix == matches.len() { 0 } else { ix })
}

/// The last match starting before `before`, wrapping to the last.
pub fn previous_match(matches: &[Range<usize>], before: usize) -> Option<usize> {
    if matches.is_empty() {
        return None;
    }
    let ix = matches.partition_point(|m| m.start < before);
    Some(ix.checked_sub(1).unwrap_or(matches.len() - 1))
}

/// Replaces every match with `replacement` as one change, selecting the end
/// of the last replacement.
pub fn replace_all(matches: &[Range<usize>], text: &str, replacement: &str) -> Option<Change> {
    let (first, last) = (matches.first()?, matches.last()?);
    let mut replaced = String::new();
    let mut at = first.start;
    for m in matches {
        replaced.push_str(&text[at..m.start]);
        replaced.push_str(replacement);
        at = m.end;
    }
    let end = first.start + replaced.len();
    Some(Change {
        range: first.start..last.end,
        text: replaced,
        selection: end..end,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_ignore_case() {
        assert_eq!(
            find_all("Focal, focal, FOCAL", "focal"),
            [0..5, 7..12, 14..19]
        );
    }

    #[test]
    fn an_empty_query_matches_nothing() {
        assert!(find_all("text", "").is_empty());
    }

    #[test]
    fn non_ascii_letters_match_across_case() {
        let text = "Ärger und ärger";
        assert_eq!(find_all(text, "ärger"), [0..6, 11..17]);
        assert!(
            find_all("Straße", "STRASSE").is_empty(),
            "no full case folding"
        );
    }

    #[test]
    fn matches_stay_on_character_boundaries() {
        // `İ` lowercases to two characters; it must neither match `i` nor panic.
        let text = "İstanbul is";
        for range in find_all(text, "i") {
            assert!(text.is_char_boundary(range.start) && text.is_char_boundary(range.end));
        }
        let found = find_all(text, "is");
        assert_eq!((found.len(), found.first()), (1, Some(&(10..12))));
    }

    #[test]
    fn a_query_can_span_lines() {
        assert_eq!(find_all("one\ntwo\none\ntwo", "one\ntwo"), [0..7, 8..15]);
    }

    #[test]
    fn matches_do_not_overlap() {
        assert_eq!(find_all("aaa", "aa").first(), Some(&(0..2)));
        assert_eq!(find_all("aaa", "aa").len(), 1);
    }

    #[test]
    fn next_and_previous_wrap() {
        let matches = [2..4, 8..10];
        assert_eq!(next_match(&matches, 0), Some(0));
        assert_eq!(next_match(&matches, 2), Some(0));
        assert_eq!(next_match(&matches, 3), Some(1));
        assert_eq!(next_match(&matches, 9), Some(0), "wraps to the first");
        assert_eq!(previous_match(&matches, 8), Some(0));
        assert_eq!(previous_match(&matches, 2), Some(1), "wraps to the last");
        assert_eq!(next_match(&[], 0), None);
        assert_eq!(previous_match(&[], 0), None);
    }

    #[test]
    fn replace_all_is_one_change() {
        let text = "a cat, a Cat, a dog";
        let matches = find_all(text, "cat");
        let change = replace_all(&matches, text, "bird").unwrap();
        assert_eq!(change.range, 2..12);
        assert_eq!(change.text, "bird, a bird");
        assert_eq!(change.selection, 14..14);
        assert_eq!(replace_all(&[], text, "bird"), None);
    }
}
