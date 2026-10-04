//! Review notes for agents, kept beside a document in `NAME.review.md`:
//! each note quotes the passage it is about and says what to change. The
//! file is plain Markdown an agent can read on its own; the document itself
//! is never touched.

use std::fmt::Write as _;
use std::ops::Range;
use std::path::{Path, PathBuf};

/// One note: the line it was made on, the passage, and what to change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Note {
    pub line: usize,
    pub quote: String,
    pub text: String,
}

/// A sidecar's notes, and anything else written in it, kept as it is.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Notes {
    pub notes: Vec<Note>,
    pub extra: String,
}

/// Where a note's passage is now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Anchor {
    Found(Range<usize>),
    /// The passage is gone (rewritten); the note stays at the start of its
    /// line.
    Changed(usize),
}

const SUFFIX: &str = ".review.md";

/// `notes.md` → `notes.review.md`, beside it.
pub fn sidecar_path(document: &Path) -> PathBuf {
    let stem = document
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    document.with_file_name(format!("{stem}{SUFFIX}"))
}

/// Whether `path` is a review sidecar rather than a document.
pub fn is_sidecar(path: &Path) -> bool {
    path.file_name()
        .map(|name| name.to_string_lossy())
        .is_some_and(|name| name.len() > SUFFIX.len() && name.ends_with(SUFFIX))
}

/// The notes in a sidecar's text.
pub fn parse(text: &str) -> Notes {
    let mut notes = Notes::default();
    let mut extra: Vec<&str> = Vec::new();
    let mut section: Option<(usize, Vec<&str>)> = None;
    for line in text.lines() {
        if let Some(number) = line
            .strip_prefix("## Line ")
            .and_then(|n| n.trim().parse().ok())
        {
            notes
                .notes
                .extend(section.take().map(|(line, body)| note(line, &body)));
            section = Some((number, Vec::new()));
        } else if let Some((_, body)) = &mut section {
            body.push(line);
        } else if !line.starts_with("# Review notes for ") {
            extra.push(line);
        }
    }
    notes
        .notes
        .extend(section.map(|(line, body)| note(line, &body)));
    extra.join("\n").trim().clone_into(&mut notes.extra);
    notes
}

/// A note from the lines under its heading: the quote, then the text.
fn note(line: usize, body: &[&str]) -> Note {
    let mut lines = body.iter().skip_while(|l| l.trim().is_empty()).peekable();
    let mut quote = Vec::new();
    while let Some(quoted) = lines.peek().and_then(|l| l.strip_prefix('>')) {
        quote.push(quoted.strip_prefix(' ').unwrap_or(quoted));
        lines.next();
    }
    let text: Vec<&str> = lines.copied().collect();
    Note {
        line,
        quote: quote.join("\n"),
        text: text.join("\n").trim().to_owned(),
    }
}

/// The sidecar's text for `notes` on the document `file_name`.
pub fn write(file_name: &str, notes: &[Note], extra: &str) -> String {
    let mut out = format!("# Review notes for {file_name}\n\n");
    if !extra.trim().is_empty() {
        out.push_str(extra.trim());
        out.push_str("\n\n");
    }
    let blocks: Vec<String> = notes
        .iter()
        .map(|note| {
            let mut block = format!("## Line {}\n\n", note.line);
            for line in note.quote.lines() {
                if line.is_empty() {
                    block.push_str(">\n");
                } else {
                    let _ = writeln!(block, "> {line}");
                }
            }
            if !note.text.trim().is_empty() {
                let _ = write!(block, "\n{}\n", note.text.trim());
            }
            block
        })
        .collect();
    out.push_str(&blocks.join("\n"));
    out
}

/// Where `note`'s passage is in `text`: the occurrence nearest the line it
/// was made on, or its line when the passage is gone.
pub fn anchor(text: &str, note: &Note) -> Anchor {
    let line_of = |offset: usize| text[..offset].matches('\n').count() + 1;
    let found = (!note.quote.is_empty())
        .then(|| {
            text.match_indices(note.quote.as_str())
                .min_by_key(|(start, _)| line_of(*start).abs_diff(note.line))
        })
        .flatten();
    if let Some((start, quote)) = found {
        return Anchor::Found(start..start + quote.len());
    }
    let starts: Vec<usize> = std::iter::once(0)
        .chain(text.match_indices('\n').map(|(at, _)| at + 1))
        .collect();
    let ix = note.line.saturating_sub(1).min(starts.len() - 1);
    Anchor::Changed(starts[ix])
}

/// What "Copy for Agent" puts on the pasteboard.
pub fn prompt(file_name: &str, notes: &[Note]) -> String {
    let sidecar = sidecar_path(Path::new(file_name));
    let mut out = format!(
        "Please revise {file_name} according to these review notes. The notes are also saved in {}; delete each note there once it is handled.\n\n",
        sidecar.display()
    );
    for (ix, note) in notes.iter().enumerate() {
        let _ = writeln!(
            out,
            "{}. Line {}: \"{}\"",
            ix + 1,
            note.line,
            note.quote.replace('\n', " ")
        );
        for line in note.text.lines() {
            let _ = writeln!(out, "   {line}");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    const SIDECAR: &str = "# Review notes for notes.md\n\n## Line 12\n\n> The fox jumps.\n\nMake it a cat.\n\n## Line 3\n\n> Two\n> lines\n\nFirst line.\nSecond line.\n";

    #[test]
    fn parse_and_write_round_trip() {
        let notes = parse(SIDECAR);
        assert_eq!(notes.notes.len(), 2);
        assert_eq!(
            notes.notes[0],
            Note {
                line: 12,
                quote: "The fox jumps.".into(),
                text: "Make it a cat.".into()
            }
        );
        assert_eq!(notes.notes[1].quote, "Two\nlines");
        assert_eq!(notes.notes[1].text, "First line.\nSecond line.");
        assert_eq!(write("notes.md", &notes.notes, &notes.extra), SIDECAR);
    }

    #[test]
    fn unknown_content_is_kept() {
        let text = "# Review notes for notes.md\n\nSome preface.\n\n## Line 1\n\n> a\n\nb\n";
        let notes = parse(text);
        assert_eq!(notes.extra, "Some preface.");
        assert_eq!(write("notes.md", &notes.notes, &notes.extra), text);
    }

    #[test]
    fn nearest_occurrence_wins() {
        let text = "fox\nx\nx\nx\nfox\n";
        let note = Note {
            line: 5,
            quote: "fox".into(),
            text: String::new(),
        };
        assert_eq!(anchor(text, &note), Anchor::Found(10..13));
    }

    #[test]
    fn a_rewritten_passage_is_changed_not_lost() {
        let note = Note {
            line: 2,
            quote: "gone".into(),
            text: "x".into(),
        };
        assert_eq!(anchor("a\nb\n", &note), Anchor::Changed(2));
        let far = Note {
            line: 99,
            quote: "gone".into(),
            text: String::new(),
        };
        assert_eq!(anchor("a\nb", &far), Anchor::Changed(2), "the last line");
    }

    #[test]
    fn umlauts_and_crlf() {
        let text = "Grüße\r\nWelt\r\n";
        let note = Note {
            line: 2,
            quote: "Welt".into(),
            text: String::new(),
        };
        assert_eq!(anchor(text, &note), Anchor::Found(9..13));
    }

    #[test]
    fn the_prompt_numbers_notes_and_names_the_sidecar() {
        let notes = [Note {
            line: 12,
            quote: "The fox jumps.".into(),
            text: "Make it a cat.".into(),
        }];
        assert_eq!(
            prompt("notes.md", &notes),
            "Please revise notes.md according to these review notes. The notes are also saved in notes.review.md; delete each note there once it is handled.\n\n1. Line 12: \"The fox jumps.\"\n   Make it a cat.\n"
        );
    }

    #[test]
    fn sidecar_paths() {
        assert_eq!(
            sidecar_path(Path::new("/a/notes.md")),
            Path::new("/a/notes.review.md")
        );
        assert!(is_sidecar(Path::new("notes.review.md")));
        assert!(!is_sidecar(Path::new("review.md")));
        assert!(!is_sidecar(Path::new(".review.md")));
    }
}
