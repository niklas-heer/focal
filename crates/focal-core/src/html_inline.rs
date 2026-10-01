//! Raw HTML in Markdown, read the way README files use it: inline
//! formatting tags, images that stand alone (also centered in a `<p
//! align>`), headings written as HTML, `<details>` with its `<summary>`,
//! and wrapper lines. Nothing here runs or renders arbitrary HTML.

use std::ops::Range;

/// A formatting element on one line: its opening tag, content and closing
/// tag, and the tag's name in lowercase.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Element {
    pub name: String,
    pub open: Range<usize>,
    pub content: Range<usize>,
    pub close: Range<usize>,
    /// The `href` of an `<a>`.
    pub href: Option<String>,
}

/// Formatting tags read in text.
pub const FORMATTING: &[&str] = &[
    "b", "strong", "i", "em", "u", "ins", "s", "del", "strike", "mark", "kbd", "sub", "sup",
    "code", "a", "small", "abbr", "span", "summary",
];

/// The formatting elements on `line`, each closed on the same line.
pub fn elements(line: &str) -> Vec<Element> {
    let lower = line.to_ascii_lowercase();
    let mut found = Vec::new();
    for (at, _) in line.match_indices('<') {
        let Some((name, end, attributes)) = open_tag(line, at) else {
            continue;
        };
        if !FORMATTING.contains(&name.as_str()) || attributes.trim_end().ends_with('/') {
            continue;
        }
        let closing = format!("</{name}>");
        if let Some(close) = lower[end..].find(&closing).map(|ix| ix + end) {
            found.push(Element {
                href: (name == "a")
                    .then(|| attribute(attributes, "href"))
                    .flatten(),
                name,
                open: at..end,
                content: end..close,
                close: close..close + closing.len(),
            });
        }
    }
    found
}

/// An opening tag at `at`: its lowercase name, where it ends, and its
/// attributes' source.
fn open_tag(line: &str, at: usize) -> Option<(String, usize, &str)> {
    let rest = &line[at + 1..];
    let name_len = rest.bytes().take_while(u8::is_ascii_alphanumeric).count();
    if name_len == 0 || !rest.as_bytes()[0].is_ascii_alphabetic() {
        return None;
    }
    let after = &rest[name_len..];
    if !after.starts_with(['>', ' ', '/', '\t']) {
        return None;
    }
    let close = after.find('>')?;
    let attributes = &after[..close];
    if attributes.contains('<') {
        return None;
    }
    let end = at + 1 + name_len + close + 1;
    Some((rest[..name_len].to_ascii_lowercase(), end, attributes))
}

/// The value of attribute `name` in `attributes`, quoted or not.
fn attribute(attributes: &str, name: &str) -> Option<String> {
    let lower = attributes.to_ascii_lowercase();
    let mut search = 0;
    while let Some(found) = lower[search..].find(name).map(|ix| ix + search) {
        search = found + name.len();
        let before_ok = found == 0 || lower.as_bytes()[found - 1].is_ascii_whitespace();
        let rest = attributes[search..].trim_start();
        let Some(value) = rest.strip_prefix('=').filter(|_| before_ok) else {
            continue;
        };
        let value = value.trim_start();
        let parsed = match value.chars().next() {
            Some(quote @ ('"' | '\'')) => value[1..].split(quote).next().unwrap_or(""),
            _ => value
                .split(|c: char| c.is_whitespace() || c == '/')
                .next()
                .unwrap_or(""),
        };
        return Some(parsed.to_owned());
    }
    None
}

/// `<br>`, `<br/>` and `<br />` on `line`.
pub fn breaks(line: &str) -> Vec<Range<usize>> {
    let lower = line.to_ascii_lowercase();
    lower
        .match_indices("<br")
        .filter_map(|(at, _)| {
            let rest = &lower[at + 3..];
            let tail = rest.trim_start_matches(' ');
            let tail = tail.strip_prefix('/').unwrap_or(tail);
            tail.starts_with('>')
                .then(|| at..at + 3 + (rest.len() - tail.len()) + 1)
        })
        .collect()
}

/// A single tag filling `line` (beyond spaces): its range and the tag.
fn sole_tag(line: &str) -> Option<(Range<usize>, &str)> {
    let start = line.len() - line.trim_start().len();
    let tag = line.trim();
    (tag.starts_with('<') && tag.ends_with('>') && tag[1..].find('<').is_none())
        .then(|| (start..start + tag.len(), tag))
}

/// An `<img>` that is all of `line` (beyond spaces): its range, `src`,
/// `alt` and `width`.
pub fn image(line: &str) -> Option<(Range<usize>, String, String, Option<u32>)> {
    let (range, tag) = sole_tag(line)?;
    let (name, _, attributes) = open_tag(tag, 0)?;
    if name != "img" {
        return None;
    }
    let src = attribute(attributes, "src").filter(|src| !src.is_empty())?;
    let alt = attribute(attributes, "alt").unwrap_or_default();
    let width = attribute(attributes, "width").and_then(|w| w.trim_end_matches("px").parse().ok());
    Some((range, src, alt, width))
}

/// A heading written as HTML, `<h2 align="center">Title</h2>`, filling the
/// line: its level and the element.
pub fn heading(line: &str) -> Option<(u8, Element)> {
    let start = line.len() - line.trim_start().len();
    let trimmed = line.trim_end();
    let (name, end, _) = open_tag(line, start).filter(|_| line[start..].starts_with('<'))?;
    let level = match name.as_str() {
        "h1" => 1,
        "h2" => 2,
        "h3" => 3,
        "h4" => 4,
        "h5" => 5,
        "h6" => 6,
        _ => return None,
    };
    let closing = format!("</{name}>");
    if !trimmed.to_ascii_lowercase().ends_with(&closing) {
        return None;
    }
    let close = trimmed.len() - closing.len();
    (close >= end).then_some((
        level,
        Element {
            name,
            open: start..end,
            content: end..close,
            close: close..trimmed.len(),
            href: None,
        },
    ))
}

/// Whether `line` is only a layout wrapper tag that Focal hides: `<p
/// align>`, `<div>`, `<center>`, `<picture>`, `<source>`, `<details>` and
/// their closing tags.
pub fn is_wrapper(line: &str) -> bool {
    let Some((_, tag)) = sole_tag(line) else {
        return false;
    };
    let name = tag[1..]
        .trim_start_matches('/')
        .split(|c: char| !c.is_ascii_alphanumeric())
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(
        name.as_str(),
        "p" | "div" | "center" | "picture" | "source" | "details"
    )
}

/// GitHub's tagfilter: `<` of the tags that could break a page (`script`,
/// `style`, `iframe` …) becomes `&lt;`.
pub fn tagfilter(html: &str) -> std::borrow::Cow<'_, str> {
    const FILTERED: &[&str] = &[
        "title",
        "textarea",
        "style",
        "xmp",
        "iframe",
        "noembed",
        "noframes",
        "script",
        "plaintext",
    ];
    let lower = html.to_ascii_lowercase();
    let mut out = String::new();
    let mut copied = 0;
    for (at, _) in lower.match_indices('<') {
        let rest = lower[at + 1..].trim_start_matches('/');
        let filtered = FILTERED.iter().any(|tag| {
            rest.starts_with(tag)
                && rest[tag.len()..]
                    .chars()
                    .next()
                    .is_none_or(|c| c.is_whitespace() || c == '>' || c == '/')
        });
        if filtered {
            out.push_str(&html[copied..at]);
            out.push_str("&lt;");
            copied = at + 1;
        }
    }
    if copied == 0 {
        return std::borrow::Cow::Borrowed(html);
    }
    out.push_str(&html[copied..]);
    std::borrow::Cow::Owned(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formatting_elements_on_a_line() {
        let line = r#"Some <b>bold</b>, <kbd>⌘</kbd>K and <a href="https://x.y">link</a>."#;
        let found = elements(line);
        let names: Vec<(&str, &str)> = found
            .iter()
            .map(|e| (e.name.as_str(), &line[e.content.clone()]))
            .collect();
        assert_eq!(names, [("b", "bold"), ("kbd", "⌘"), ("a", "link")]);
        assert_eq!(&line[found[0].open.clone()], "<b>");
        assert_eq!(&line[found[0].close.clone()], "</b>");
        assert_eq!(found[2].href.as_deref(), Some("https://x.y"));
    }

    #[test]
    fn unclosed_and_unknown_tags_are_not_elements() {
        assert!(elements("<b>never closed").is_empty());
        assert!(elements("<blink>no</blink>").is_empty());
        assert!(elements("a < b and c > d").is_empty());
    }

    #[test]
    fn line_breaks() {
        let line = "one<br>two<br/>three<br />four";
        let found: Vec<&str> = breaks(line).into_iter().map(|r| &line[r]).collect();
        assert_eq!(found, ["<br>", "<br/>", "<br />"]);
    }

    #[test]
    fn an_image_alone_on_its_line() {
        let (range, src, alt, width) =
            image(r#"  <img src="logo.png" alt="Logo" width="120">"#).unwrap();
        assert_eq!(
            (range.start, src.as_str(), alt.as_str(), width),
            (2, "logo.png", "Logo", Some(120))
        );
        assert!(image(r#"Text <img src="a.png"> more"#).is_none());
        assert!(image("<img alt=\"no source\">").is_none());
    }

    #[test]
    fn headings_written_as_html() {
        let line = r#"<h1 align="center">Focal</h1>"#;
        let (level, element) = heading(line).unwrap();
        assert_eq!((level, &line[element.content]), (1, "Focal"));
        assert_eq!(
            (&line[element.open], &line[element.close]),
            (r#"<h1 align="center">"#, "</h1>")
        );
        assert!(heading("<h2>Open").is_none());
    }

    #[test]
    fn wrapper_lines() {
        for line in [
            r#"<p align="center">"#,
            "</p>",
            "<div>",
            "</div>",
            "<center>",
            "  </center>",
            "<details>",
            "<details open>",
            "</details>",
            "<picture>",
            r#"<source srcset="x.png">"#,
        ] {
            assert!(is_wrapper(line), "{line}");
        }
        assert!(!is_wrapper("<p>text</p>"));
        assert!(!is_wrapper("text"));
    }

    #[test]
    fn the_tagfilter_defuses_dangerous_tags() {
        assert_eq!(tagfilter("<script>x</script>"), "&lt;script>x&lt;/script>");
        assert_eq!(tagfilter("<IFRAME src=x>"), "&lt;IFRAME src=x>");
        assert_eq!(tagfilter("<b>fine</b>"), "<b>fine</b>");
    }
}
