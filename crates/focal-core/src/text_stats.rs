//! Word counts, reading time and sentences, for the bottom bar, focus mode
//! and the info panel.

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

/// What the info panel counts in a text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Statistics {
    pub words: usize,
    /// Characters as people see them, line endings left out.
    pub characters: usize,
    pub characters_without_spaces: usize,
    pub sentences: usize,
    /// Runs of lines that are not blank.
    pub paragraphs: usize,
    pub reading_minutes: usize,
}

pub fn statistics(text: &str) -> Statistics {
    let mut characters = 0;
    let mut blanks = 0;
    for grapheme in text.graphemes(true) {
        if grapheme == "\n" || grapheme == "\r\n" || grapheme == "\r" {
            continue;
        }
        characters += 1;
        if grapheme.chars().all(char::is_whitespace) {
            blanks += 1;
        }
    }
    let mut paragraphs = 0;
    let mut sentences = 0;
    let mut paragraph = String::new();
    for line in text.lines().chain(std::iter::once("")) {
        if line.trim().is_empty() {
            if !paragraph.is_empty() {
                paragraphs += 1;
                sentences += paragraph
                    .unicode_sentences()
                    .filter(|sentence| sentence.unicode_words().next().is_some())
                    .count();
                paragraph.clear();
            }
        } else {
            paragraph.push_str(line);
            paragraph.push(' ');
        }
    }
    let words = word_count(text);
    Statistics {
        words,
        characters,
        characters_without_spaces: characters - blanks,
        sentences,
        paragraphs,
        reading_minutes: reading_minutes(words),
    }
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
    fn statistics_count_what_a_reader_sees() {
        let stats = statistics("# Title\n\nOne two. Three!\r\nStill one.\n\n- é\n");
        assert_eq!(stats.words, 7);
        assert_eq!(stats.paragraphs, 3);
        assert_eq!(stats.sentences, 5, "the title and the item are one each");
        assert_eq!(stats.characters, 35, "line endings are not characters");
        assert_eq!(stats.characters_without_spaces, 30);
        assert_eq!(statistics(""), Statistics::default());
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
