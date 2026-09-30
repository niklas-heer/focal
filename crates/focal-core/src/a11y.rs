//! Text for accessibility clients. AccessKit describes text as runs of at most
//! 255 characters, each with its characters' byte lengths and word starts.

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

/// AccessKit indexes characters with `u8`.
const MAX_CHARACTERS: usize = 255;

/// One AccessKit text run: a slice of a line with its character and word data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextChunk {
    /// Byte range in the text the chunks were made from.
    pub range: Range<usize>,
    /// Byte length of each character (grapheme cluster).
    pub character_lengths: Vec<u8>,
    /// Index of each character that starts a word.
    pub word_starts: Vec<u8>,
}

/// Splits `text` into chunks of at most 255 characters, preferring to split
/// where a word starts.
pub fn text_chunks(text: &str) -> Vec<TextChunk> {
    // (byte start, byte length) of each character. A cluster longer than a
    // `u8` can describe is split into its scalar values.
    let mut characters: Vec<(usize, usize)> = Vec::new();
    for (start, cluster) in text.grapheme_indices(true) {
        if u8::try_from(cluster.len()).is_ok() {
            characters.push((start, cluster.len()));
        } else {
            characters.extend(
                cluster
                    .char_indices()
                    .map(|(ix, ch)| (start + ix, ch.len_utf8())),
            );
        }
    }
    let mut word_start = vec![false; characters.len()];
    if let Some(first) = word_start.first_mut() {
        *first = true;
    }
    let mut next = 0;
    for (start, segment) in text.split_word_bound_indices() {
        while next < characters.len() && characters[next].0 < start {
            next += 1;
        }
        if next < characters.len()
            && characters[next].0 == start
            && segment.chars().any(char::is_alphanumeric)
        {
            word_start[next] = true;
        }
    }

    let mut chunks = Vec::new();
    let mut first = 0;
    while first < characters.len() {
        let mut end = (first + MAX_CHARACTERS).min(characters.len());
        if end < characters.len()
            && let Some(start) = (first + 1..=end).rev().find(|&ix| word_start[ix])
        {
            end = start;
        }
        let range = characters[first].0..characters.get(end).map_or(text.len(), |c| c.0);
        let as_u8 = |value: usize| u8::try_from(value).unwrap_or(u8::MAX);
        chunks.push(TextChunk {
            range,
            character_lengths: characters[first..end].iter().map(|c| as_u8(c.1)).collect(),
            word_starts: (first..end)
                .filter(|&ix| word_start[ix])
                .map(|ix| as_u8(ix - first))
                .collect(),
        });
        first = end;
    }
    chunks
}

/// The chunk index and character index of a byte offset. The end of the text
/// is the position after the last character.
pub fn position_of(chunks: &[TextChunk], offset: usize) -> (usize, usize) {
    let Some(last) = chunks.len().checked_sub(1) else {
        return (0, 0);
    };
    let chunk = chunks
        .iter()
        .position(|chunk| offset < chunk.range.end)
        .unwrap_or(last);
    let mut position = chunks[chunk].range.start;
    let mut character = 0;
    for length in &chunks[chunk].character_lengths {
        if position >= offset {
            break;
        }
        position += usize::from(*length);
        character += 1;
    }
    (chunk, character)
}

/// The byte offset of a character index in a chunk.
pub fn offset_of(chunks: &[TextChunk], chunk: usize, character: usize) -> usize {
    chunks.get(chunk).map_or(0, |chunk| {
        chunk.range.start
            + chunk
                .character_lengths
                .iter()
                .take(character)
                .map(|&length| usize::from(length))
                .sum::<usize>()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn characters_are_grapheme_clusters() {
        let chunks = text_chunks("né 🇩🇪\n");
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].character_lengths, [1, 2, 1, 8, 1]);
        assert_eq!(chunks[0].range, 0..13);
    }

    #[test]
    fn words_start_at_letters_and_digits() {
        let chunks = text_chunks("  Hello, world 42.\n");
        // "  " is its own word; "Hello, " and "world " keep their trailing punctuation and spaces.
        assert_eq!(chunks[0].word_starts, [0, 2, 9, 15]);
    }

    #[test]
    fn long_text_splits_at_255_characters_on_word_boundaries() {
        let text = "word ".repeat(100);
        let chunks = text_chunks(&text);
        assert!(chunks.iter().all(|c| c.character_lengths.len() <= 255));
        assert_eq!(
            chunks.iter().map(|c| c.range.len()).sum::<usize>(),
            text.len()
        );
        assert_eq!(chunks[1].range.start % 5, 0, "splits between words");
        assert_eq!(chunks[1].word_starts.first(), Some(&0));
    }

    #[test]
    fn empty_text_has_no_chunks() {
        assert!(text_chunks("").is_empty());
    }

    #[test]
    fn maps_offsets_to_positions_and_back() {
        let text = "ab".repeat(200);
        let chunks = text_chunks(&text);
        let (chunk, character) = position_of(&chunks, 300);
        assert_eq!((chunk, character), (1, 300 - chunks[1].range.start));
        assert_eq!(offset_of(&chunks, chunk, character), 300);
        assert_eq!(
            position_of(&chunks, text.len()),
            (
                chunks.len() - 1,
                chunks[chunks.len() - 1].character_lengths.len()
            )
        );
    }
}
