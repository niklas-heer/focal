//! The shadow text: an equal-length copy of the source in which other
//! Markdown dialects' syntax is rewritten into what `pulldown-cmark`
//! understands. Every byte keeps its offset, so parsing the shadow gives
//! ranges into the untouched source.

use std::borrow::Cow;

/// `text` with other dialects' syntax rewritten, byte for byte the same
/// length. Code spans and fenced code are left alone.
pub fn shadow(text: &str) -> Cow<'_, str> {
    let mut out = text.as_bytes().to_vec();
    let lines = line_ranges(text);
    // The open code fence: its character, its length and whether it is math.
    let mut fence: Option<(u8, usize, bool)> = None;
    for (ix, line) in lines.iter().enumerate() {
        let start = line.start + prefix_len(&text[line.clone()]);
        let body = &text[start..line.end];
        if let Some((ch, count, math)) = fence {
            if closes_fence(body, ch, count) {
                if math {
                    overwrite_fence(&mut out, start, body);
                }
                fence = None;
            }
            continue;
        }
        if let Some((ch, count, info)) = opening_fence(body) {
            let math = info.trim().eq_ignore_ascii_case("math");
            if math {
                overwrite_fence(&mut out, start, body);
            }
            fence = Some((ch, count, math));
            continue;
        }
        inline(text, &mut out, &lines, ix, start);
    }
    if out == text.as_bytes() {
        Cow::Borrowed(text)
    } else {
        // Only ASCII bytes were replaced by ASCII bytes.
        Cow::Owned(String::from_utf8(out).unwrap_or_else(|_| text.to_owned()))
    }
}

/// Each line's range, without its line ending.
fn line_ranges(text: &str) -> Vec<std::ops::Range<usize>> {
    let mut lines = Vec::new();
    let mut start = 0;
    for (ix, byte) in text.bytes().enumerate() {
        if byte == b'\n' {
            let end = if ix > start && text.as_bytes()[ix - 1] == b'\r' {
                ix - 1
            } else {
                ix
            };
            lines.push(start..end);
            start = ix + 1;
        }
    }
    lines.push(start..text.len());
    lines
}

/// The length of a line's indentation and `>` quote markers.
fn prefix_len(line: &str) -> usize {
    let bytes = line.as_bytes();
    let mut at = 0;
    while at < bytes.len() && (bytes[at] == b' ' || bytes[at] == b'>' || bytes[at] == b'\t') {
        at += 1;
    }
    at
}

/// A fence that opens a code block: its character, length and info string.
fn opening_fence(body: &str) -> Option<(u8, usize, &str)> {
    let ch = *body.as_bytes().first()?;
    if ch != b'`' && ch != b'~' {
        return None;
    }
    let count = body.bytes().take_while(|&b| b == ch).count();
    let info = &body[count..];
    (count >= 3 && !(ch == b'`' && info.contains('`'))).then_some((ch, count, info))
}

fn closes_fence(body: &str, ch: u8, count: usize) -> bool {
    let run = body.bytes().take_while(|&b| b == ch).count();
    run >= count && body[run..].trim().is_empty()
}

/// A math fence line becomes `$$` followed by spaces.
fn overwrite_fence(out: &mut [u8], start: usize, body: &str) {
    let len = body.trim_end().len();
    for (ix, byte) in out[start..start + len].iter_mut().enumerate() {
        *byte = if ix < 2 { b'$' } else { b' ' };
    }
}

/// Rewrites the math delimiters on line `ix`, from `start` on.
fn inline(text: &str, out: &mut [u8], lines: &[std::ops::Range<usize>], ix: usize, start: usize) {
    let bytes = text.as_bytes();
    let end = lines[ix].end;
    let mut at = start;
    while at < end {
        match bytes[at] {
            b'\\' if at + 1 < end => {
                match bytes[at + 1] {
                    b'(' => {
                        if let Some(close) = find_escaped(bytes, at + 2, end, b')') {
                            out[at..at + 2].copy_from_slice(b" $");
                            out[close..close + 2].copy_from_slice(b"$ ");
                            at = close + 2;
                            continue;
                        }
                    }
                    b'[' if text[start..at].trim().is_empty() => {
                        if let Some(close) = display_close(text, lines, ix, at + 2) {
                            out[at..at + 2].copy_from_slice(b"$$");
                            out[close..close + 2].copy_from_slice(b"$$");
                            at = close + 2;
                            continue;
                        }
                    }
                    _ => {}
                }
                // Any other escape, an escaped backslash included.
                at += 2;
            }
            b'$' if at + 1 < end && bytes[at + 1] == b'`' => match text[at + 2..end].find("`$") {
                Some(offset) => {
                    let close = at + 2 + offset;
                    out[at..at + 2].copy_from_slice(b" $");
                    out[close..close + 2].copy_from_slice(b"$ ");
                    at = close + 2;
                }
                None => at += 2,
            },
            b'`' => {
                // A code span: skip to its closing run of the same length.
                let run = bytes[at..end].iter().take_while(|&&b| b == b'`').count();
                let ticks = &text[at..at + run];
                at = match text[at + run..end].find(ticks) {
                    Some(offset) => at + run + offset + run,
                    None => at + run,
                };
            }
            _ => at += 1,
        }
    }
}

/// The next unescaped `\` followed by `close` in `bytes[from..end]`.
fn find_escaped(bytes: &[u8], from: usize, end: usize, close: u8) -> Option<usize> {
    let mut at = from;
    while at + 1 < end {
        if bytes[at] == b'\\' {
            if bytes[at + 1] == close {
                return Some(at);
            }
            at += 2;
        } else {
            at += 1;
        }
    }
    None
}

/// Where the `\]` closing display math opened on line `ix` is: at the end of
/// this line or a later one, before a blank line.
fn display_close(
    text: &str,
    lines: &[std::ops::Range<usize>],
    ix: usize,
    from: usize,
) -> Option<usize> {
    let bytes = text.as_bytes();
    for (n, line) in lines.iter().enumerate().skip(ix) {
        let begin = if n == ix { from } else { line.start };
        let content = &text[begin..line.end];
        if n > ix && content.trim().is_empty() {
            return None;
        }
        let trimmed = content.trim_end();
        if trimmed.ends_with("\\]") {
            let close = begin + trimmed.len() - 2;
            // Not an escaped backslash before the bracket.
            if close == 0 || bytes[close - 1] != b'\\' {
                return Some(close);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(text: &str) -> String {
        let shadowed = shadow(text).into_owned();
        assert_eq!(shadowed.len(), text.len(), "same length: {text:?}");
        shadowed
    }

    #[test]
    fn plain_markdown_is_borrowed_unchanged() {
        assert!(matches!(shadow("# Title\n\nText $x$.\n"), Cow::Borrowed(_)));
    }

    #[test]
    fn parenthesis_math_becomes_dollars() {
        assert_eq!(check(r"where \(x^2\) holds"), "where  $x^2$  holds");
    }

    #[test]
    fn bracket_math_becomes_display_math() {
        assert_eq!(check("\\[\nE = mc^2\n\\]\n"), "$$\nE = mc^2\n$$\n");
        assert_eq!(check(r"\[ a + b \]"), r"$$ a + b $$");
    }

    #[test]
    fn unpaired_or_escaped_delimiters_stay() {
        assert_eq!(check(r"just \( an opening"), r"just \( an opening");
        assert_eq!(
            check(r"an escaped \\(x\\) pair"),
            r"an escaped \\(x\\) pair"
        );
        assert_eq!(check(r"\[not math] text"), r"\[not math] text");
    }

    #[test]
    fn gitlab_backtick_math_becomes_dollars() {
        assert_eq!(check("Inline $`a^2`$ here"), "Inline  $a^2$  here");
    }

    #[test]
    fn math_fences_become_display_math() {
        assert_eq!(check("```math\nx = 1\n```\n"), "$$     \nx = 1\n$$ \n");
        assert_eq!(check("~~~ math\nx\n~~~"), "$$      \nx\n$$ ");
    }

    #[test]
    fn code_is_left_alone() {
        let fenced = "```\n\\(x\\)\n```\n";
        assert_eq!(check(fenced), fenced);
        let span = r"see `\(x\)` and \(y\)";
        assert_eq!(check(span), r"see `\(x\)` and  $y$ ");
    }
}
