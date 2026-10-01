//! Spell checking with macOS's own checker (`NSSpellChecker`), so Focal uses
//! the same dictionaries, languages and learned words as every other Mac app.

use std::collections::HashSet;
use std::ops::Range;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_app_kit::NSSpellChecker;
use objc2_foundation::{
    NSArray, NSOrthography, NSRange, NSString, NSTextCheckingResult, NSTextCheckingType, NSValue,
};

/// A grammar issue Apple's checker found: its byte range, what it says,
/// and its suggested corrections.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GrammarIssue {
    pub range: Range<usize>,
    pub description: String,
    pub corrections: Vec<String>,
}

/// Whether macOS's "Correct spelling automatically" is on, for every app.
pub fn system_corrects_spelling() -> bool {
    NSSpellChecker::isAutomaticSpellingCorrectionEnabled()
}

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

    /// Byte ranges of the misspelled words in `text`, checked in the
    /// languages the text is written in.
    pub fn misspellings(&self, text: &str) -> Vec<Range<usize>> {
        let offsets = Utf16Offsets::new(text);
        let (results, _) = self.check(text, NSTextCheckingType::Spelling);
        results
            .iter()
            .filter(|result| result.resultType() == NSTextCheckingType::Spelling)
            .map(|result| {
                let range = result.range();
                offsets.to_bytes(range.location)..offsets.to_bytes(range.location + range.length)
            })
            .filter(|bytes| !self.ignored.contains(&text[bytes.clone()]))
            .collect()
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

    /// The correction macOS would make on its own for the word at `range`
    /// (bytes) in `text`, when the word is misspelled in the language `text`
    /// is written in.
    pub fn correction(&self, text: &str, range: Range<usize>) -> Option<String> {
        if self.ignored.contains(&text[range.clone()]) {
            return None;
        }
        let word = NSRange::new(
            text[..range.start].encode_utf16().count(),
            text[range].encode_utf16().count(),
        );
        // A word alone is easily taken for another language ("saw" for
        // German), so the language comes from the whole text.
        let (results, orthography) = self.check(text, NSTextCheckingType::Spelling);
        let misspelled = results.iter().any(|result| {
            result.resultType() == NSTextCheckingType::Spelling && result.range() == word
        });
        if !misspelled {
            return None;
        }
        let language =
            orthography.map_or_else(|| self.checker.language(), |o| o.dominantLanguage());
        self.checker
            .correctionForWordRange_inString_language_inSpellDocumentWithTag(
                word,
                &NSString::from_str(text),
                &language,
                self.tag,
            )
            .map(|word| word.to_string())
    }

    /// Checks all of `text` for `types`, and the orthography (languages and
    /// scripts) the checker found it written in.
    #[allow(unsafe_code)]
    fn check(
        &self,
        text: &str,
        types: NSTextCheckingType,
    ) -> (
        Retained<NSArray<NSTextCheckingResult>>,
        Option<Retained<NSOrthography>>,
    ) {
        let string = NSString::from_str(text);
        let whole = NSRange::new(0, text.encode_utf16().count());
        let mut orthography = None;
        // SAFETY: no options dictionary (it would need the documented key and
        // value types), and a null word count, which the method accepts in
        // place of a pointer.
        let results = unsafe {
            self.checker
                .checkString_range_types_options_inSpellDocumentWithTag_orthography_wordCount(
                    &string,
                    whole,
                    types.0,
                    None,
                    self.tag,
                    Some(&mut orthography),
                    std::ptr::null_mut(),
                )
        };
        (results, orthography)
    }

    /// The grammar issues in `text`, from the same checker as spelling.
    pub fn grammar(&self, text: &str) -> Vec<GrammarIssue> {
        let offsets = Utf16Offsets::new(text);
        // Grammar results only come with spelling checked too.
        let (results, _) = self.check(
            text,
            NSTextCheckingType(NSTextCheckingType::Grammar.0 | NSTextCheckingType::Spelling.0),
        );
        let key = |name: &str| NSString::from_str(name);
        let mut issues = Vec::new();
        for result in &results {
            if result.resultType() != NSTextCheckingType::Grammar {
                continue;
            }
            let base = result.range().location;
            for detail in result.grammarDetails().iter().flat_map(|d| d.iter()) {
                let Some(range) = detail
                    .objectForKey(&key("NSGrammarRange"))
                    .and_then(|value| value.downcast::<NSValue>().ok())
                    .and_then(|value| value.get_range())
                else {
                    continue;
                };
                let description = detail
                    .objectForKey(&key("NSGrammarUserDescription"))
                    .and_then(|value| value.downcast::<NSString>().ok())
                    .map(|value| value.to_string())
                    .unwrap_or_default();
                let corrections = detail
                    .objectForKey(&key("NSGrammarCorrections"))
                    .and_then(|value| value.downcast::<NSArray<AnyObject>>().ok())
                    .map(|list| {
                        list.iter()
                            .filter_map(|item| item.downcast::<NSString>().ok())
                            .map(|item| item.to_string())
                            .collect()
                    })
                    .unwrap_or_default();
                let start = base + range.location;
                issues.push(GrammarIssue {
                    range: offsets.to_bytes(start)..offsets.to_bytes(start + range.length),
                    description,
                    corrections,
                });
            }
        }
        issues
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
    fn finds_misspelled_words_in_the_texts_language() {
        let checker = SpellChecker::new();
        let text = "I saw teh cat.";
        assert_eq!(words(text, &checker.misspellings(text)), ["teh"]);
    }

    #[test]
    fn finds_misspelled_words_as_byte_ranges() {
        let checker = SpellChecker::new();
        // Multi-byte characters that no dictionary judges, before the words.
        let text = "→ “thsi” is a sentance with a tpyo.";
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

    #[test]
    fn finds_grammar_issues_with_corrections() {
        let checker = SpellChecker::new();
        let text = "I has a apple.";
        let issues = checker.grammar(text);
        let issue = issues
            .iter()
            .find(|i| &text[i.range.clone()] == "a")
            .expect("a/an");
        assert!(issue.corrections.contains(&"an".to_owned()), "{issue:?}");
        assert!(!issue.description.is_empty());
        assert!(checker.grammar("A fine sentence.").is_empty());
    }

    #[test]
    fn corrects_a_typo_like_macos_would() {
        let checker = SpellChecker::new();
        let text = "I saw teh cat.";
        assert_eq!(checker.correction(text, 6..9).as_deref(), Some("the"));
        assert_eq!(checker.correction("I saw the cat.", 6..9), None);
        assert_eq!(
            checker.correction("I saw teh cat.", 2..5),
            None,
            "a word is checked in its sentence's language"
        );
    }
}
