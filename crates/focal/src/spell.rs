//! Spell checking with macOS's own checker (`NSSpellChecker`), so Focal uses
//! the same dictionaries, languages and learned words as every other Mac app.

use std::collections::HashSet;
use std::ops::Range;

use objc2::rc::Retained;
use objc2_app_kit::NSSpellChecker;
use objc2_foundation::{NSRange, NSString};

pub struct SpellChecker {
    checker: Retained<NSSpellChecker>,
    tag: isize,
    /// Words ignored for this session with "Ignore Spelling".
    ignored: HashSet<String>,
}

impl SpellChecker {
    pub fn new() -> Self {
        Self {
            checker: NSSpellChecker::sharedSpellChecker(),
            tag: NSSpellChecker::uniqueSpellDocumentTag(),
            ignored: HashSet::new(),
        }
    }

    /// Byte ranges of the misspelled words in `text`.
    pub fn misspellings(&self, text: &str) -> Vec<Range<usize>> {
        let string = NSString::from_str(text);
        let offsets = Utf16Offsets::new(text);
        let mut found = Vec::new();
        let mut start = 0;
        while start < offsets.len() {
            let Ok(from) = isize::try_from(start) else {
                break;
            };
            let range = self.checker.checkSpellingOfString_startingAt(&string, from);
            // The checker wraps around to the start; stop once it does.
            if range.length == 0 || range.location < start || range.location >= offsets.len() {
                break;
            }
            start = range.location + range.length;
            let bytes = offsets.to_bytes(range.location)..offsets.to_bytes(start);
            if !self.ignored.contains(&text[bytes.clone()]) {
                found.push(bytes);
            }
        }
        found
    }

    /// Suggested replacements for the word at `range` (bytes) in `text`.
    pub fn guesses(&self, text: &str, range: Range<usize>) -> Vec<String> {
        let string = NSString::from_str(text);
        let start = text[..range.start].encode_utf16().count();
        let length = text[range].encode_utf16().count();
        let range = NSRange::new(start, length);
        // With automatic language identification, a short phrase on a German Mac
        // is checked as German and gets no English suggestions, so also ask in
        // each of the user's preferred spelling languages.
        let languages = self.checker.userPreferredLanguages();
        let mut guesses: Vec<String> = Vec::new();
        let candidates = std::iter::once(None).chain(languages.iter().map(Some));
        for language in candidates {
            let found = self
                .checker
                .guessesForWordRange_inString_language_inSpellDocumentWithTag(
                    range,
                    &string,
                    language.as_deref(),
                    self.tag,
                );
            for guess in found.iter().flat_map(|found| found.iter()) {
                let guess = guess.to_string();
                if !guesses.contains(&guess) {
                    guesses.push(guess);
                }
            }
        }
        guesses
    }

    pub fn ignore(&mut self, word: &str) {
        self.ignored.insert(word.to_owned());
    }

    /// Adds the word to the user's dictionary, shared with all apps.
    pub fn learn(&self, word: &str) {
        self.checker.learnWord(&NSString::from_str(word));
    }
}

/// Converts `NSString`'s UTF-16 offsets back to byte offsets.
struct Utf16Offsets {
    /// Byte offset for each UTF-16 offset, plus one for the end.
    bytes: Vec<usize>,
}

impl Utf16Offsets {
    fn new(text: &str) -> Self {
        let mut bytes = Vec::with_capacity(text.len() + 1);
        for (offset, ch) in text.char_indices() {
            for _ in 0..ch.len_utf16() {
                bytes.push(offset);
            }
        }
        bytes.push(text.len());
        Self { bytes }
    }

    /// The text's length in UTF-16 code units.
    fn len(&self) -> usize {
        self.bytes.len() - 1
    }

    fn to_bytes(&self, utf16: usize) -> usize {
        self.bytes
            .get(utf16)
            .copied()
            .unwrap_or_else(|| self.bytes[self.len()])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words<'t>(text: &'t str, ranges: &[Range<usize>]) -> Vec<&'t str> {
        ranges.iter().map(|r| &text[r.clone()]).collect()
    }

    #[test]
    fn finds_misspelled_words_as_byte_ranges() {
        let checker = SpellChecker::new();
        let text = "Grüße: thsi is a sentance with a tpyo.";
        assert_eq!(
            words(text, &checker.misspellings(text)),
            ["thsi", "sentance", "tpyo"]
        );
    }

    #[test]
    fn suggests_corrections() {
        let checker = SpellChecker::new();
        let text = "a sentance";
        let guesses = checker.guesses(text, 2..10);
        assert!(guesses.iter().any(|g| g == "sentence"), "{guesses:?}");
    }

    #[test]
    fn ignored_words_are_not_reported() {
        let mut checker = SpellChecker::new();
        checker.ignore("Focalish");
        assert!(checker.misspellings("Focalish words").is_empty());
    }
}
