//! Parts of speech from macOS's `NaturalLanguage` framework, for iA
//! Writer-style syntax highlighting, and the language a paragraph is
//! written in, for the style check.
//!
//! One of the named macOS bridges where `unsafe` is allowed (decision
//! "Allow unsafe code only in named macOS bridges"): every `NLTagger`
//! method is `unsafe` in `objc2-natural-language`. It is confined to [`tag`].

use std::ops::Range;

use objc2::AnyThread as _;
use objc2_foundation::{NSArray, NSRange, NSString};
use objc2_natural_language::{
    NLTagAdjective, NLTagAdverb, NLTagConjunction, NLTagNoun, NLTagSchemeLexicalClass, NLTagVerb,
    NLTagger, NLTokenUnit,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Part {
    Adjective,
    Noun,
    Adverb,
    Verb,
    Conjunction,
}

impl Part {
    pub const ALL: [Self; 5] = [
        Self::Adjective,
        Self::Noun,
        Self::Adverb,
        Self::Verb,
        Self::Conjunction,
    ];

    /// The style that marks the part in a line's runs.
    pub const fn style(self) -> focal_core::analysis::InlineStyle {
        use focal_core::analysis::InlineStyle;
        match self {
            Self::Adjective => InlineStyle::ADJECTIVE,
            Self::Noun => InlineStyle::NOUN,
            Self::Adverb => InlineStyle::ADVERB,
            Self::Verb => InlineStyle::VERB,
            Self::Conjunction => InlineStyle::CONJUNCTION,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Adjective => "Adjectives",
            Self::Noun => "Nouns",
            Self::Adverb => "Adverbs",
            Self::Verb => "Verbs",
            Self::Conjunction => "Conjunctions",
        }
    }
}

/// A paragraph's words that are one of the [`Part`]s, as byte ranges, and
/// whether it is written in German.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tagged {
    pub parts: Vec<(Range<usize>, Part)>,
    pub german: bool,
}

/// Tags `text`, one paragraph, with its parts of speech.
#[allow(unsafe_code)]
pub fn tag(text: &str) -> Tagged {
    let mut result = Tagged::default();
    if text.trim().is_empty() {
        return result;
    }
    let string = NSString::from_str(text);
    let length = string.length();
    // SAFETY: the statics are `NaturalLanguage`'s own constants, set by the
    // framework before any code runs; reading them has no side effects.
    let (Some(scheme), kinds) = (unsafe { NLTagSchemeLexicalClass }, unsafe {
        [
            (NLTagAdjective, Part::Adjective),
            (NLTagNoun, Part::Noun),
            (NLTagAdverb, Part::Adverb),
            (NLTagVerb, Part::Verb),
            (NLTagConjunction, Part::Conjunction),
        ]
    }) else {
        return result;
    };
    // SAFETY: a fresh tagger, used on this thread only, given an array of
    // one valid scheme and a valid string it retains.
    let tagger = unsafe {
        let tagger =
            NLTagger::initWithTagSchemes(NLTagger::alloc(), &NSArray::from_slice(&[scheme]));
        tagger.setString(Some(&string));
        tagger
    };
    // SAFETY: `dominantLanguage` reads the tagger's analysis of its string.
    result.german = unsafe { tagger.dominantLanguage() }
        .is_some_and(|language| language.to_string().starts_with("de"));
    let mut index = 0;
    while index < length {
        let mut range = NSRange::new(0, 0);
        // SAFETY: `index` is inside the string, and `range` is a valid
        // pointer to an `NSRange` that lives across the call.
        let found = unsafe {
            tagger.tagAtIndex_unit_scheme_tokenRange(
                index,
                NLTokenUnit::Word,
                scheme,
                &raw mut range,
            )
        };
        if range.length == 0 || range.location + range.length <= index {
            index += 1;
            continue;
        }
        index = range.location + range.length;
        let Some(found) = found else { continue };
        let part = kinds
            .iter()
            .find(|(kind, _)| kind.is_some_and(|kind| kind.isEqualToString(&found)))
            .map(|(_, part)| *part);
        if let Some(part) = part {
            let start = byte_offset(text, range.location);
            let end = byte_offset(text, range.location + range.length);
            result.parts.push((start..end, part));
        }
    }
    result
}

/// The byte offset in `text` of a UTF-16 offset, as `NaturalLanguage` counts.
fn byte_offset(text: &str, utf16: usize) -> usize {
    let mut units = 0;
    for (ix, c) in text.char_indices() {
        if units >= utf16 {
            return ix;
        }
        units += c.len_utf16();
    }
    text.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(text: &str, part: Part) -> Vec<&str> {
        tag(text)
            .parts
            .iter()
            .filter(|(_, p)| *p == part)
            .map(|(range, _)| &text[range.clone()])
            .collect()
    }

    #[test]
    fn english_parts_of_speech() {
        let text = "The quick fox jumps over the lazy dog, and the cat sleeps quietly.";
        assert_eq!(words(text, Part::Adjective), ["quick", "lazy"]);
        assert!(words(text, Part::Noun).contains(&"fox"));
        assert!(words(text, Part::Verb).contains(&"sleeps"));
        assert_eq!(words(text, Part::Adverb), ["quietly"]);
        assert_eq!(words(text, Part::Conjunction), ["and"]);
        assert!(!tag(text).german);
    }

    #[test]
    fn german_is_recognized_and_offsets_are_bytes() {
        let text = "Die große Füchsin 🦊 springt schnell über den Zaun.";
        let tagged = tag(text);
        assert!(tagged.german);
        assert!(words(text, Part::Noun).contains(&"Füchsin"));
        for (range, _) in &tagged.parts {
            assert!(text.is_char_boundary(range.start) && text.is_char_boundary(range.end));
        }
    }

    #[test]
    fn utf16_offsets_become_bytes() {
        let text = "Grüße 🦊 fox";
        assert_eq!(byte_offset(text, 0), 0);
        assert_eq!(byte_offset(text, 6), 8, "after ü and ß, two bytes each");
        assert_eq!(
            byte_offset(text, 9),
            13,
            "the fox emoji is two UTF-16 units"
        );
        assert_eq!(byte_offset(text, 99), text.len());
    }
}
