//! Word counts, reading time and sentences, for the bottom bar and focus mode.

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation as _;

/// Words by Unicode rules; Markdown markers such as `#`, `**` and `|` are
/// punctuation, not words.
pub fn word_count(text: &str) -> usize {
    text.unicode_words().count()
}

/// Minutes to read `words` at 230 words a minute, rounded up.
pub fn reading_minutes(words: usize) -> usize {
    words.div_ceil(230)
}

/// The sentence of the paragraph `range` that contains `offset`, without its
/// trailing whitespace. An offset in the space after a sentence belongs to it.
pub fn sentence_at(text: &str, range: Range<usize>, offset: usize) -> Range<usize> {
    let paragraph = &text[range.clone()];
    let at = offset.clamp(range.start, range.end) - range.start;
    let mut found = 0..paragraph.len();
    for (start, sentence) in paragraph.split_sentence_bound_indices() {
        found = start..start + sentence.len();
        if at < found.end {
            break;
        }
    }
    let trimmed = paragraph[found.clone()].trim_end().len();
    range.start + found.start..range.start + found.start + trimmed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markers_are_not_words() {
        assert_eq!(word_count("# A **bold** move\n\n- one\n| x | y |\n"), 6);
        assert_eq!(word_count("don't stop — it's 3.5 km"), 5);
        assert_eq!(word_count(""), 0);
    }

    #[test]
    fn reading_takes_at_least_a_minute() {
        assert_eq!(reading_minutes(0), 0);
        assert_eq!(reading_minutes(1), 1);
        assert_eq!(reading_minutes(460), 2);
        assert_eq!(reading_minutes(461), 3);
    }

    #[test]
    fn the_sentence_around_the_caret() {
        let text = "> First one. Second one? Third.";
        let paragraph = 2..text.len();
        assert_eq!(&text[sentence_at(text, paragraph.clone(), 4)], "First one.");
        assert_eq!(
            &text[sentence_at(text, paragraph.clone(), 15)],
            "Second one?"
        );
        assert_eq!(
            &text[sentence_at(text, paragraph.clone(), text.len())],
            "Third."
        );
        assert_eq!(
            &text[sentence_at(text, paragraph, 12)],
            "First one.",
            "the space after a sentence belongs to it"
        );
    }
}
