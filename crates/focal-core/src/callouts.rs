//! Callouts in every dialect: Obsidian's `> [!type] Title` (GitHub's five
//! alerts among them), fenced containers `::: type Title` (Docusaurus,
//! VitePress, Pandoc, Quarto), GitLab's `>>>` quotes and MkDocs admonitions
//! `!!! type "Title"` with an indented body.

use std::ops::Range;

use crate::analysis::Alert;
use crate::shadow::{closes_fence, line_ranges, opening_fence, prefix_len};

/// How a callout is written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Style {
    /// `> [!type]` on a quote's first line.
    Quote,
    /// Between fence lines; `closing` is the closing fence's line, if any.
    Fenced { closing: Option<usize> },
    /// `!!! type` with a four-space indented body.
    Indented,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Callout {
    pub style: Style,
    /// Its lines, fences and title line included.
    pub lines: Range<usize>,
    /// The color family; `None` for a plain quote or block.
    pub alert: Option<Alert>,
    /// Whether this is a known kind of callout (a titled block), rather than
    /// a plain `>>>` quote or a Pandoc div of another class.
    pub titled: bool,
    /// What the title reads: the title written in the source, or the kind's
    /// name.
    pub title: String,
    /// On the first line, the syntax hidden away from the caret: the whole
    /// line, or with a written title in a quote callout, the `[!type]` part.
    pub head: Range<usize>,
    /// The written title in the source, shown as it stands (quote callouts).
    pub title_range: Option<Range<usize>>,
    /// Whether it folds, and if so whether it starts folded: Obsidian's `-`
    /// and `+`, MkDocs' `???` and `???+`, VitePress' `details`.
    pub folded: Option<bool>,
}

/// The color family and default title of a callout type, if it is one.
pub fn kind(name: &str) -> Option<(Option<Alert>, &'static str)> {
    let name = name.trim().to_ascii_lowercase();
    let name = name.strip_prefix("callout-").unwrap_or(&name);
    let found = match name {
        "note" => (Some(Alert::Note), "Note"),
        "info" => (Some(Alert::Note), "Info"),
        "todo" => (Some(Alert::Note), "Todo"),
        "abstract" => (Some(Alert::Note), "Abstract"),
        "summary" => (Some(Alert::Note), "Summary"),
        "tldr" => (Some(Alert::Note), "TL;DR"),
        "seealso" | "see-also" => (Some(Alert::Note), "See also"),
        "tip" => (Some(Alert::Tip), "Tip"),
        "hint" => (Some(Alert::Tip), "Hint"),
        "success" => (Some(Alert::Tip), "Success"),
        "check" => (Some(Alert::Tip), "Check"),
        "done" => (Some(Alert::Tip), "Done"),
        "important" => (Some(Alert::Important), "Important"),
        "question" => (Some(Alert::Important), "Question"),
        "help" => (Some(Alert::Important), "Help"),
        "faq" => (Some(Alert::Important), "FAQ"),
        "example" => (Some(Alert::Important), "Example"),
        "warning" => (Some(Alert::Warning), "Warning"),
        "attention" => (Some(Alert::Warning), "Attention"),
        "caution" => (Some(Alert::Caution), "Caution"),
        "danger" => (Some(Alert::Caution), "Danger"),
        "error" => (Some(Alert::Caution), "Error"),
        "bug" => (Some(Alert::Caution), "Bug"),
        "failure" | "fail" => (Some(Alert::Caution), "Failure"),
        "missing" => (Some(Alert::Caution), "Missing"),
        "quote" => (None, "Quote"),
        "cite" => (None, "Cite"),
        "details" => (None, "Details"),
        _ => return None,
    };
    Some(found)
}

/// The callouts of `text`, outside code.
pub fn callouts(text: &str) -> Vec<Callout> {
    let lines = line_ranges(text);
    let mut found = Vec::new();
    let mut open: Vec<Open> = Vec::new();
    let mut fence: Option<(u8, usize)> = None;
    let mut ix = 0;
    while ix < lines.len() {
        let line = lines[ix].clone();
        let source = &text[line.clone()];
        let quoted = prefix_len(source);
        let indent = source.len() - source.trim_start_matches(' ').len();
        let body = &source[indent..];
        if let Some((ch, count)) = fence {
            if closes_fence(&source[quoted..], ch, count) {
                fence = None;
            }
            ix += 1;
            continue;
        }
        if let Some((ch, count, _)) = opening_fence(&source[quoted..]) {
            fence = Some((ch, count));
            ix += 1;
            continue;
        }
        if indent <= 3 {
            if let Some(colons) = colon_fence(body) {
                let rest = body[colons..].trim();
                if rest.is_empty() {
                    if open
                        .last()
                        .is_some_and(|o| o.colons.is_some_and(|c| colons >= c))
                    {
                        let closed = open.pop().unwrap_or_default();
                        found.push(closed.finish(ix, ix + 1));
                    }
                } else {
                    open.push(container(rest, colons, ix, line.clone()));
                }
                ix += 1;
                continue;
            }
            if body.trim_end() == ">>>" {
                if open.last().is_some_and(|o| o.colons.is_none()) {
                    let closed = open.pop().unwrap_or_default();
                    found.push(closed.finish(ix, ix + 1));
                } else {
                    open.push(Open {
                        start: ix,
                        head: line.clone(),
                        ..Open::default()
                    });
                }
                ix += 1;
                continue;
            }
            if let Some(callout) = admonition(text, &lines, ix) {
                ix = callout.lines.end;
                found.push(callout);
                continue;
            }
        }
        if let Some(callout) = quote_callout(text, &lines, ix) {
            ix = callout.lines.end;
            found.push(callout);
            continue;
        }
        ix += 1;
    }
    // Unclosed containers run to the last line with text.
    let last = lines
        .iter()
        .rposition(|line| !text[line.clone()].trim().is_empty())
        .map_or(0, |line| line + 1);
    while let Some(unclosed) = open.pop() {
        let end = last.max(unclosed.start + 1);
        let mut callout = unclosed.finish(end, end);
        callout.style = Style::Fenced { closing: None };
        found.push(callout);
    }
    found.sort_by_key(|callout| callout.lines.start);
    found
}

/// A container being read.
#[derive(Default)]
struct Open {
    start: usize,
    /// The fence's colons; `None` for GitLab's `>>>`.
    colons: Option<usize>,
    alert: Option<Alert>,
    titled: bool,
    title: String,
    head: Range<usize>,
    folded: Option<bool>,
}

impl Open {
    fn finish(self, closing: usize, end: usize) -> Callout {
        Callout {
            style: Style::Fenced {
                closing: Some(closing),
            },
            lines: self.start..end,
            alert: self.alert,
            titled: self.titled,
            title: self.title,
            head: self.head,
            title_range: None,
            folded: self.folded,
        }
    }
}

/// The number of colons opening or closing a container, at least three.
fn colon_fence(body: &str) -> Option<usize> {
    let colons = body.bytes().take_while(|&b| b == b':').count();
    (colons >= 3).then_some(colons)
}

/// An opening container fence: `type`, `type Title`, `type[Title]` or a
/// Pandoc attribute list `{.class title="Title"}`.
fn container(rest: &str, colons: usize, ix: usize, line: Range<usize>) -> Open {
    let (name, written) = if let Some(attributes) = rest.strip_prefix('{') {
        let attributes = attributes.trim_end_matches('}');
        let name = attributes
            .split_whitespace()
            .find_map(|word| word.strip_prefix('.'))
            .unwrap_or("");
        let title = attributes
            .split_once("title=\"")
            .and_then(|(_, after)| after.split_once('"'))
            .map(|(title, _)| title.to_owned());
        (name.to_owned(), title)
    } else {
        let name_len = rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
            .unwrap_or(rest.len());
        let (name, after) = rest.split_at(name_len);
        let title = after
            .strip_prefix('[')
            .and_then(|t| t.split_once(']'))
            .map_or(after, |(t, _)| t)
            .trim()
            .trim_matches('"');
        (
            name.to_owned(),
            (!title.is_empty()).then(|| title.to_owned()),
        )
    };
    let known = kind(&name);
    Open {
        start: ix,
        colons: Some(colons),
        alert: known.and_then(|(alert, _)| alert),
        titled: known.is_some(),
        title: written
            .or_else(|| known.map(|(_, title)| title.to_owned()))
            .unwrap_or_default(),
        head: line,
        folded: (name.eq_ignore_ascii_case("details")).then_some(true),
    }
}

/// `!!! type "Title"`, `??? type` or `???+ type` and its indented body.
fn admonition(text: &str, lines: &[Range<usize>], ix: usize) -> Option<Callout> {
    let line = lines[ix].clone();
    let source = &text[line.clone()];
    let (rest, folded) = [("!!!", None), ("???+", Some(false)), ("???", Some(true))]
        .iter()
        .find_map(|(opener, folded)| source.strip_prefix(opener).map(|rest| (rest, *folded)))?;
    let rest = rest.strip_prefix(' ')?.trim();
    let (name, after) = rest.split_once(' ').unwrap_or((rest, ""));
    if name.is_empty()
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return None;
    }
    let written = after
        .split_once('"')
        .and_then(|(_, t)| t.split_once('"'))
        .map(|(t, _)| t.to_owned())
        .filter(|t| !t.is_empty());
    let known = kind(name);
    let mut end = ix + 1;
    let mut last = ix + 1;
    while end < lines.len() {
        let body = &text[lines[end].clone()];
        if body.trim().is_empty() {
            end += 1;
            continue;
        }
        if !(body.starts_with("    ") || body.starts_with('\t')) {
            break;
        }
        end += 1;
        last = end;
    }
    let mut title = written.unwrap_or_else(|| match known {
        Some((_, title)) => title.to_owned(),
        None => capitalized(name),
    });
    if title.is_empty() {
        title = capitalized(name);
    }
    Some(Callout {
        style: Style::Indented,
        lines: ix..last,
        alert: known.and_then(|(alert, _)| alert).or(if known.is_none() {
            Some(Alert::Note)
        } else {
            None
        }),
        titled: true,
        title,
        head: line,
        title_range: None,
        folded,
    })
}

/// `> [!type]±? Title` and the quote's following `>` lines.
fn quote_callout(text: &str, lines: &[Range<usize>], ix: usize) -> Option<Callout> {
    let line = lines[ix].clone();
    let source = &text[line.clone()];
    let indent = source.len() - source.trim_start_matches(' ').len();
    if indent > 3 || !source[indent..].starts_with('>') {
        return None;
    }
    let mut at = indent + 1;
    if source[at..].starts_with(' ') {
        at += 1;
    }
    let after = source[at..].strip_prefix("[!")?;
    let close = after.find(']')?;
    let (alert, default) = kind(&after[..close])?;
    let head_start = line.start + at;
    let mut head_end = head_start + 2 + close + 1;
    let folded = match text[head_end..line.end].chars().next() {
        Some('-') => Some(true),
        Some('+') => Some(false),
        _ => None,
    };
    if folded.is_some() {
        head_end += 1;
    }
    let written = text[head_end..line.end].trim();
    let title_range = (!written.is_empty()).then(|| {
        let start = head_end
            + (text[head_end..line.end].len() - text[head_end..line.end].trim_start().len());
        start..start + written.len()
    });
    if let Some(title) = &title_range {
        head_end = title.start;
    }
    let mut end = ix + 1;
    while end < lines.len() {
        let next = &text[lines[end].clone()];
        let next_indent = next.len() - next.trim_start_matches(' ').len();
        if next_indent > 3 || !next[next_indent..].starts_with('>') {
            break;
        }
        end += 1;
    }
    Some(Callout {
        style: Style::Quote,
        lines: ix..end,
        alert,
        titled: true,
        title: title_range.as_ref().map_or_else(
            || default.to_owned(),
            |range| text[range.clone()].to_owned(),
        ),
        head: head_start..head_end,
        title_range,
        folded,
    })
}

fn capitalized(name: &str) -> String {
    let mut chars = name.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn only(text: &str) -> Callout {
        let found = callouts(text);
        assert_eq!(found.len(), 1, "{text:?}: {found:?}");
        found.into_iter().next().unwrap()
    }

    #[test]
    fn obsidian_callouts_with_any_type_title_and_fold() {
        let text = "> [!info]- My *title*\n> Body\n\nAfter\n";
        let callout = only(text);
        assert_eq!(callout.style, Style::Quote);
        assert_eq!(callout.lines, 0..2);
        assert_eq!(callout.alert, Some(Alert::Note));
        assert_eq!(callout.title, "My *title*");
        assert_eq!(&text[callout.head.clone()], "[!info]- ");
        assert_eq!(&text[callout.title_range.unwrap()], "My *title*");
    }

    #[test]
    fn a_callout_without_a_title_reads_its_kind() {
        let text = "> [!danger]\n> Hot\n";
        let callout = only(text);
        assert_eq!(
            (callout.alert, callout.title.as_str()),
            (Some(Alert::Caution), "Danger")
        );
        assert_eq!(&text[callout.head], "[!danger]");
        assert_eq!(callout.title_range, None);
    }

    #[test]
    fn plain_quotes_and_unknown_types_are_not_callouts() {
        assert!(callouts("> just a quote\n").is_empty());
        assert!(callouts("> [!whatever] hm\n").is_empty());
        assert!(callouts("> [link](x)\n").is_empty());
    }

    #[test]
    fn fenced_containers_in_their_spellings() {
        for (text, alert, title) in [
            (
                "::: warning Careful\nBody\n:::\n",
                Some(Alert::Warning),
                "Careful",
            ),
            (":::tip\nBody\n:::\n", Some(Alert::Tip), "Tip"),
            (
                ":::note[Docusaurus title]\nBody\n:::\n",
                Some(Alert::Note),
                "Docusaurus title",
            ),
            (
                "::: {.callout-important}\nBody\n:::\n",
                Some(Alert::Important),
                "Important",
            ),
            ("::: details Click me\nBody\n:::\n", None, "Click me"),
        ] {
            let callout = only(text);
            assert_eq!(
                callout.style,
                Style::Fenced { closing: Some(2) },
                "{text:?}"
            );
            assert_eq!(
                (callout.alert, callout.title.as_str()),
                (alert, title),
                "{text:?}"
            );
            assert!(callout.titled, "{text:?}");
            assert_eq!(callout.lines, 0..3, "{text:?}");
        }
    }

    #[test]
    fn nested_and_unclosed_containers() {
        let text = ":::: note\nOuter\n::: tip\nInner\n:::\n::::\n";
        let found = callouts(text);
        assert_eq!(found.len(), 2, "{found:?}");
        assert_eq!(found[0].lines, 0..6);
        assert_eq!(found[1].lines, 2..5);
        let open = only("::: note\nnever closed\n\nstill inside\n");
        assert_eq!(open.style, Style::Fenced { closing: None });
        assert_eq!(open.lines, 0..4);
    }

    #[test]
    fn other_pandoc_divs_are_untitled_blocks() {
        let callout = only("::: {.columns}\nA\n:::\n");
        assert!(!callout.titled);
        assert_eq!(callout.alert, None);
    }

    #[test]
    fn gitlab_multiline_quotes() {
        let callout = only(">>>\nQuoted\n\nStill quoted\n>>>\n");
        assert_eq!(callout.style, Style::Fenced { closing: Some(4) });
        assert!(!callout.titled);
        assert_eq!(callout.lines, 0..5);
    }

    #[test]
    fn mkdocs_admonitions_take_their_indented_body() {
        let text = "!!! note \"Read this\"\n    First.\n\n    Second.\n\nAfter\n";
        let callout = only(text);
        assert_eq!(callout.style, Style::Indented);
        assert_eq!(callout.lines, 0..4);
        assert_eq!(
            (callout.alert, callout.title.as_str()),
            (Some(Alert::Note), "Read this")
        );
        assert_eq!(only("???+ tip\n    Body\n").title, "Tip");
    }

    #[test]
    fn callout_syntax_in_code_is_ignored() {
        assert!(callouts("```\n::: note\n> [!tip]\n!!! note\n```\n").is_empty());
    }

    #[test]
    fn fold_markers() {
        assert_eq!(only("> [!faq]- Why\n> x\n").folded, Some(true));
        assert_eq!(only("> [!faq]+ Why\n> x\n").folded, Some(false));
        assert_eq!(only("> [!faq] Why\n> x\n").folded, None);
        assert_eq!(only("??? note\n    x\n").folded, Some(true));
        assert_eq!(only("???+ note\n    x\n").folded, Some(false));
        assert_eq!(only("!!! note\n    x\n").folded, None);
        assert_eq!(only("::: details More\nx\n:::\n").folded, Some(true));
    }
}
