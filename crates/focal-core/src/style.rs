//! A style check after iA Writer's: filler words, clichés and redundancies,
//! in English and German, found as whole words in a line of prose.

use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IssueKind {
    /// A word that adds little: "very", "eigentlich".
    Filler,
    /// A worn phrase: "at the end of the day", "unterm Strich".
    Cliche,
    /// Saying a thing twice: "end result", "bereits schon".
    Redundancy,
}

impl IssueKind {
    /// What the context menu calls it.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Filler => "Filler word",
            Self::Cliche => "Cliché",
            Self::Redundancy => "Redundancy",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Issue {
    pub range: Range<usize>,
    pub kind: IssueKind,
}

const EN_FILLERS: &[&str] = &[
    "very",
    "really",
    "just",
    "quite",
    "actually",
    "basically",
    "literally",
    "simply",
    "totally",
    "completely",
    "absolutely",
    "definitely",
    "certainly",
    "extremely",
    "somewhat",
    "rather",
    "fairly",
    "truly",
    "honestly",
    "obviously",
    "clearly",
    "essentially",
    "practically",
    "virtually",
    "kind of",
    "sort of",
    "a little bit",
    "in fact",
    "needless to say",
];

const EN_CLICHES: &[&str] = &[
    "at the end of the day",
    "in this day and age",
    "think outside the box",
    "low-hanging fruit",
    "move the needle",
    "at this point in time",
    "last but not least",
    "few and far between",
    "avoid like the plague",
    "game changer",
    "a perfect storm",
    "tip of the iceberg",
    "back to square one",
    "hit the ground running",
    "the elephant in the room",
    "only time will tell",
    "it goes without saying",
    "when all is said and done",
    "best of both worlds",
    "take it to the next level",
    "paradigm shift",
    "circle back",
    "level playing field",
    "push the envelope",
];

const EN_REDUNDANCIES: &[&str] = &[
    "end result",
    "past history",
    "free gift",
    "added bonus",
    "final outcome",
    "advance planning",
    "advance warning",
    "basic fundamentals",
    "close proximity",
    "completely finished",
    "each and every",
    "future plans",
    "join together",
    "new innovation",
    "true facts",
    "unexpected surprise",
    "revert back",
    "repeat again",
    "still remains",
    "absolutely essential",
    "collaborate together",
    "combine together",
    "exact same",
    "general consensus",
    "mutual cooperation",
    "past experience",
    "personal opinion",
    "plan ahead",
    "regular routine",
    "sum total",
    "unintended mistake",
    "atm machine",
    "pin number",
    "first began",
    "merge together",
    "brief summary",
    "true fact",
];

const DE_FILLERS: &[&str] = &[
    "eigentlich",
    "halt",
    "eben",
    "wirklich",
    "ziemlich",
    "quasi",
    "sozusagen",
    "irgendwie",
    "total",
    "echt",
    "einfach",
    "natürlich",
    "gewissermaßen",
    "durchaus",
    "letztendlich",
    "regelrecht",
    "im Grunde",
    "an sich",
    "praktisch",
    "absolut",
    "schlichtweg",
    "sicherlich",
    "im Prinzip",
    "an und für sich",
];

const DE_CLICHES: &[&str] = &[
    "am Ende des Tages",
    "unterm Strich",
    "nach wie vor",
    "in der heutigen Zeit",
    "Hand und Fuß",
    "auf Augenhöhe",
    "im Endeffekt",
    "ins Boot holen",
    "an einem Strang ziehen",
    "über den Tellerrand",
    "das A und O",
    "mit Fug und Recht",
    "in trockenen Tüchern",
    "auf den Punkt bringen",
    "Licht am Ende des Tunnels",
    "Nägel mit Köpfen",
    "last but not least",
];

const DE_REDUNDANCIES: &[&str] = &[
    "bereits schon",
    "neu renoviert",
    "weißer Schimmel",
    "nochmals wiederholen",
    "zusammen addieren",
    "schon vorher",
    "bisher noch nie",
    "meistens immer",
    "einzelne Individuen",
    "Rückantwort",
    "Zukunftsprognose",
    "gegenseitige Kooperation",
    "wieder zurück",
    "grundsätzlich immer",
    "LCD-Display",
    "HIV-Virus",
    "persönlich anwesend",
    "vorher ankündigen",
    "zuerst beginnen",
    "gemeinsam zusammenarbeiten",
];

/// The style issues in `text` (a line of prose), in order: whole words or
/// phrases, ignoring case, from the German lists when `german`, otherwise
/// the English ones. Longer phrases win over the words inside them.
pub fn issues(text: &str, german: bool) -> Vec<Issue> {
    let lists: [(&[&str], IssueKind); 3] = if german {
        [
            (DE_FILLERS, IssueKind::Filler),
            (DE_CLICHES, IssueKind::Cliche),
            (DE_REDUNDANCIES, IssueKind::Redundancy),
        ]
    } else {
        [
            (EN_FILLERS, IssueKind::Filler),
            (EN_CLICHES, IssueKind::Cliche),
            (EN_REDUNDANCIES, IssueKind::Redundancy),
        ]
    };
    let mut phrases: Vec<(&str, IssueKind)> = lists
        .iter()
        .flat_map(|(list, kind)| list.iter().map(move |phrase| (*phrase, *kind)))
        .collect();
    phrases.sort_by_key(|(phrase, _)| std::cmp::Reverse(phrase.len()));
    let mut found: Vec<Issue> = Vec::new();
    for (phrase, kind) in phrases {
        for range in crate::find::find_all(text, phrase) {
            let whole = !text[..range.start]
                .chars()
                .next_back()
                .is_some_and(char::is_alphanumeric)
                && !text[range.end..]
                    .chars()
                    .next()
                    .is_some_and(char::is_alphanumeric);
            let overlaps = found
                .iter()
                .any(|issue| issue.range.start < range.end && range.start < issue.range.end);
            if whole && !overlaps {
                found.push(Issue { range, kind });
            }
        }
    }
    found.sort_by_key(|issue| issue.range.start);
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(text: &str, german: bool) -> Vec<(&str, IssueKind)> {
        issues(text, german)
            .into_iter()
            .map(|issue| (&text[issue.range], issue.kind))
            .collect()
    }

    #[test]
    fn english_fillers_cliches_and_redundancies() {
        assert_eq!(
            found(
                "It was very late. The end result, at the end of the day, was fine.",
                false
            ),
            [
                ("very", IssueKind::Filler),
                ("end result", IssueKind::Redundancy),
                ("at the end of the day", IssueKind::Cliche)
            ]
        );
    }

    #[test]
    fn whole_words_only_ignoring_case() {
        assert_eq!(
            found("Every veryfine thing. VERY good.", false),
            [("VERY", IssueKind::Filler)]
        );
    }

    #[test]
    fn german_lists_and_umlauts() {
        assert_eq!(
            found(
                "Das ist eigentlich bereits schon fertig, ganz schön übel.",
                true
            ),
            [
                ("eigentlich", IssueKind::Filler),
                ("bereits schon", IssueKind::Redundancy)
            ]
        );
    }

    #[test]
    fn the_kind_says_what_it_is() {
        assert_eq!(IssueKind::Filler.label(), "Filler word");
        assert_eq!(IssueKind::Cliche.label(), "Cliché");
        assert_eq!(IssueKind::Redundancy.label(), "Redundancy");
    }
}
