//! Markdown to HTML, for Export as HTML and Copy as HTML, in the dialect the
//! editor shows. What needs the app (typeset math, Mermaid diagrams, where a
//! wiki link points) comes from a callback.

use pulldown_cmark::{CodeBlockKind, CowStr, Event, LinkType, Parser, Tag, TagEnd};

use std::fmt::Write as _;

use crate::analysis::Alert;

/// Something the app renders for the export.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Embed<'a> {
    DisplayMath(&'a str),
    InlineMath(&'a str),
    /// A Mermaid diagram's source.
    Diagram(&'a str),
    /// A wiki link's target name; the answer is the link's `href`.
    WikiLink(&'a str),
    /// An image's source; the answer replaces it.
    Image(&'a str),
}

/// `text` as an HTML fragment. `render` turns embeds into HTML (or an
/// `href`); `None` falls back to the escaped source.
pub fn to_html(text: &str, render: &mut dyn FnMut(Embed<'_>) -> Option<String>) -> String {
    use crate::callouts::{Style, callouts};
    let prepared = prepare(text);
    let text = prepared.as_str();
    let lines = crate::shadow::line_ranges(text);
    let mut html = String::new();
    // Callouts are rendered on their own, each body as Markdown of its own.
    let mut at_line = 0;
    for callout in callouts(text) {
        if callout.lines.start < at_line {
            continue; // Inside a callout already rendered.
        }
        html.push_str(&markdown(
            &join(text, &lines, at_line..callout.lines.start),
            render,
        ));
        let body_lines = match callout.style {
            Style::Quote | Style::Indented => callout.lines.start + 1..callout.lines.end,
            Style::Fenced { closing } => {
                callout.lines.start + 1..closing.unwrap_or(callout.lines.end)
            }
        };
        let mut body = String::new();
        if callout.style == Style::Quote {
            // Text after the title on the first line belongs to the body too.
            for line in body_lines.clone() {
                body.push_str(strip_quote(&text[lines[line].clone()]));
                body.push('\n');
            }
        } else if callout.style == Style::Indented {
            for line in body_lines.clone() {
                let source = &text[lines[line].clone()];
                let stripped = source
                    .strip_prefix("    ")
                    .or_else(|| source.strip_prefix('\t'))
                    .unwrap_or(source);
                body.push_str(stripped);
                body.push('\n');
            }
        } else {
            body = join(text, &lines, body_lines.clone());
        }
        let inner = to_html(&body, render);
        if callout.titled {
            let class = callout.alert.map_or("quote", |alert| alert.class());
            let icon = callout.alert.map_or("❝", Alert::icon);
            let title = inline(&callout.title, render);
            let _ = write!(
                html,
                "<div class=\"callout callout-{class}\">\n<p class=\"callout-title\">{icon} {title}</p>\n{inner}</div>\n"
            );
        } else if text[lines[callout.lines.start].clone()]
            .trim_start()
            .starts_with('>')
        {
            let _ = write!(html, "<blockquote>\n{inner}</blockquote>\n");
        } else {
            html.push_str(&inner);
        }
        at_line = callout.lines.end;
    }
    html.push_str(&markdown(&join(text, &lines, at_line..lines.len()), render));
    html
}

/// `text` without Obsidian block comments (`%%` lines and what is between
/// them), and with each table-of-contents marker replaced by the outline
/// as HTML.
fn prepare(text: &str) -> String {
    let lines = crate::shadow::line_ranges(text);
    let outline = crate::outline::outline(text);
    let mut out = String::with_capacity(text.len());
    let mut fence: Option<(u8, usize)> = None;
    let mut commented = false;
    for line in &lines {
        let source = &text[line.clone()];
        let body = source.trim_start();
        if let Some((ch, count)) = fence {
            if crate::shadow::closes_fence(body, ch, count) {
                fence = None;
            }
        } else if let Some((ch, count, _)) = crate::shadow::opening_fence(body) {
            fence = Some((ch, count));
        } else if body.trim_end() == "%%" {
            commented = !commented;
            out.push('\n');
            continue;
        } else if commented {
            out.push('\n');
            continue;
        } else if matches!(
            body.trim().to_ascii_lowercase().as_str(),
            "[toc]" | "[[_toc_]]" | "[[toc]]" | "{:toc}"
        ) {
            out.push('\n');
            out.push_str(&toc(&outline));
            out.push_str("\n\n");
            continue;
        }
        out.push_str(source);
        out.push('\n');
    }
    out.pop();
    out
}

/// The outline as a navigation list, each entry linking to its heading.
fn toc(outline: &[crate::outline::Heading]) -> String {
    let mut html = String::from("<nav class=\"toc\"><ul>");
    for heading in outline {
        let _ = write!(
            html,
            "<li class=\"toc-{}\"><a href=\"#{}\">{}</a></li>",
            heading.level,
            crate::links::heading_slug(&heading.title),
            escape(&heading.title)
        );
    }
    html.push_str("</ul></nav>");
    html
}

/// The source of `lines`, each followed by a line break.
fn join(text: &str, lines: &[std::ops::Range<usize>], range: std::ops::Range<usize>) -> String {
    let mut joined = String::new();
    for line in range {
        joined.push_str(&text[lines[line].clone()]);
        joined.push('\n');
    }
    joined
}

/// A quote line without its `>` and the space after it.
fn strip_quote(line: &str) -> &str {
    let trimmed = line.trim_start_matches(' ');
    let unquoted = trimmed.strip_prefix('>').unwrap_or(trimmed);
    unquoted.strip_prefix(' ').unwrap_or(unquoted)
}

/// Markdown `text` as inline HTML, without the paragraph around it.
fn inline(text: &str, render: &mut dyn FnMut(Embed<'_>) -> Option<String>) -> String {
    let html = markdown(text, render);
    let html = html.trim();
    html.strip_prefix("<p>")
        .and_then(|h| h.strip_suffix("</p>"))
        .unwrap_or(html)
        .to_owned()
}

/// `text` without callouts, as HTML.
fn markdown(text: &str, render: &mut dyn FnMut(Embed<'_>) -> Option<String>) -> String {
    let (text, abbreviations) = abbreviations(text);
    let mut events: Vec<Event> = Vec::new();
    // Text read so far: `pulldown-cmark` splits text at `^`, `=` and other
    // delimiters, so extras are found in the joined text.
    let mut pending = String::new();
    // A Mermaid block's source while it is read.
    let mut diagram: Option<String> = None;
    let mut in_metadata = false;
    let mut in_code = false;
    // The open heading, by its index in `events`, and its text so far.
    let mut heading: Option<(usize, String)> = None;
    // An `![[…]]` embed being replaced: of a note (a link) or of an image
    // with a width (an `<img>`), until its end.
    let mut embed: Option<bool> = None;
    let shadowed = crate::shadow::shadow(&text);
    for event in Parser::new_ext(&shadowed, crate::analysis::options()) {
        if let Some((_, title)) = &mut heading
            && let Event::Text(text) | Event::Code(text) = &event
        {
            title.push_str(text);
        }
        if let Some(note) = embed {
            if matches!(event, Event::End(TagEnd::Image)) {
                embed = None;
                if note {
                    events.push(Event::End(TagEnd::Link));
                }
            } else if note {
                events.push(event);
            }
            continue;
        }
        if let Event::Text(text) = &event
            && !in_code
            && !in_metadata
            && diagram.is_none()
        {
            pending.push_str(text);
            continue;
        }
        if !pending.is_empty() {
            events.extend(inline_extras(&std::mem::take(&mut pending), &abbreviations));
        }
        match event {
            Event::Start(Tag::Heading { .. }) => {
                heading = Some((events.len(), String::new()));
                events.push(event);
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some((ix, title)) = heading.take() {
                    anchor(&mut events[ix], &title);
                }
                events.push(event);
            }
            Event::Start(Tag::Image {
                link_type: link_type @ LinkType::WikiLink { has_pothole },
                dest_url,
                title,
                id,
            }) => {
                let note = !crate::analysis::is_image_file(&dest_url);
                events.push(wiki_embed(
                    &shadowed,
                    link_type,
                    has_pothole,
                    &dest_url,
                    title,
                    id,
                    render,
                ));
                embed = Some(note);
            }
            Event::Start(Tag::MetadataBlock(_)) => in_metadata = true,
            Event::End(TagEnd::MetadataBlock(_)) => in_metadata = false,
            _ if in_metadata => {}
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(lang)))
                if lang.split_whitespace().next() == Some("mermaid") =>
            {
                diagram = Some(String::new());
            }
            Event::Text(source) if diagram.is_some() => {
                if let Some(diagram) = &mut diagram {
                    diagram.push_str(&source);
                }
            }
            Event::End(TagEnd::CodeBlock) if diagram.is_some() => {
                let source = diagram.take().unwrap_or_default();
                events.push(diagram_html(&source, render));
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                in_code = true;
                events.push(Event::Start(Tag::CodeBlock(kind)));
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code = false;
                events.push(Event::End(TagEnd::CodeBlock));
            }
            other => events.push(embedded(other, render)),
        }
    }
    if !pending.is_empty() {
        events.extend(inline_extras(&pending, &abbreviations));
    }
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, events.into_iter());
    html
}

/// Math, wiki links and images as the app renders or resolves them; other
/// events unchanged.
fn embedded<'a>(
    event: Event<'a>,
    render: &mut dyn FnMut(Embed<'_>) -> Option<String>,
) -> Event<'a> {
    match event {
        Event::InlineMath(tex) => {
            let html = match render(Embed::InlineMath(&tex)) {
                Some(svg) => format!(r#"<span class="math">{svg}</span>"#),
                None => format!(r#"<code class="math">{}</code>"#, escape(&tex)),
            };
            Event::InlineHtml(html.into())
        }
        Event::DisplayMath(tex) => {
            let html = match render(Embed::DisplayMath(&tex)) {
                Some(svg) => format!(r#"<div class="math">{svg}</div>"#),
                None => format!(r#"<pre class="math">{}</pre>"#, escape(&tex)),
            };
            Event::Html(html.into())
        }
        Event::Start(Tag::Link {
            link_type: link_type @ LinkType::WikiLink { .. },
            dest_url,
            title,
            id,
        }) => {
            let href =
                render(Embed::WikiLink(&dest_url)).unwrap_or_else(|| format!("{dest_url}.md"));
            Event::Start(Tag::Link {
                link_type,
                dest_url: href.into(),
                title,
                id,
            })
        }
        Event::Start(Tag::Image {
            link_type,
            dest_url,
            title,
            id,
        }) => {
            let source = render(Embed::Image(&dest_url)).map_or(dest_url, CowStr::from);
            Event::Start(Tag::Image {
                link_type,
                dest_url: source,
                title,
                id,
            })
        }
        other => other,
    }
}

/// A Mermaid diagram drawn by the app, or its source when it cannot be.
fn diagram_html(
    source: &str,
    render: &mut dyn FnMut(Embed<'_>) -> Option<String>,
) -> Event<'static> {
    let html = match render(Embed::Diagram(source)) {
        Some(svg) => format!(r#"<figure class="diagram">{svg}</figure>"#),
        None => format!(
            r#"<pre><code class="language-mermaid">{}</code></pre>"#,
            escape(source)
        ),
    };
    Event::Html(html.into())
}

/// Gives a heading without an id GitHub's anchor for `title`, so a table of
/// contents can link to it.
fn anchor(start: &mut Event<'_>, title: &str) {
    if let Event::Start(Tag::Heading { id: id @ None, .. }) = start {
        *id = Some(crate::links::heading_slug(title.trim()).into());
    }
}

/// An Obsidian embed: `![[note]]` as the start of a link to the note,
/// `![[pic.png|300]]` as an `<img>` with its width.
fn wiki_embed<'a>(
    shadowed: &str,
    link_type: LinkType,
    has_pothole: bool,
    dest_url: &str,
    title: CowStr<'a>,
    id: CowStr<'a>,
    render: &mut dyn FnMut(Embed<'_>) -> Option<String>,
) -> Event<'a> {
    if !crate::analysis::is_image_file(dest_url) {
        let href = render(Embed::WikiLink(dest_url)).unwrap_or_else(|| format!("{dest_url}.md"));
        return Event::Start(Tag::Link {
            link_type,
            dest_url: href.into(),
            title,
            id,
        });
    }
    let source = render(Embed::Image(dest_url)).unwrap_or_else(|| dest_url.to_owned());
    let width = shadowed_width(shadowed, has_pothole, dest_url);
    Event::InlineHtml(
        format!(
            r#"<img src="{}" alt="{}"{}>"#,
            escape(&source),
            escape(dest_url),
            width.map_or_else(String::new, |w| format!(r#" width="{w}""#))
        )
        .into(),
    )
}

/// The width asked for by an image embed `![[dest|300]]`, read from the
/// source around it.
fn shadowed_width(text: &str, has_pothole: bool, destination: &str) -> Option<u32> {
    if !has_pothole {
        return None;
    }
    let after = text.split(&format!("[[{destination}|")).nth(1)?;
    let written = after.split("]]").next()?;
    written.split('x').next()?.trim().parse().ok()
}

/// Abbreviation definitions `*[HTML]: Hyper Text Markup Language`, taken
/// out of `text`.
fn abbreviations(text: &str) -> (std::borrow::Cow<'_, str>, Vec<(String, String)>) {
    let mut found = Vec::new();
    let mut kept = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        let definition = line
            .trim_start()
            .strip_prefix("*[")
            .and_then(|rest| rest.split_once("]:"))
            .filter(|(word, _)| !word.is_empty());
        match definition {
            Some((word, title)) => {
                found.push((word.to_owned(), title.trim().to_owned()));
                kept.push('\n');
            }
            None => kept.push_str(line),
        }
    }
    if found.is_empty() {
        (std::borrow::Cow::Borrowed(text), found)
    } else {
        (std::borrow::Cow::Owned(kept), found)
    }
}

/// Text or markup, while text is split into extras.
enum Piece {
    Text(String),
    Html(String),
}

/// What `pulldown-cmark` does not read in text: `==highlight==` (as
/// `<mark>`), superscript `^x^` in words, and abbreviations (as `<abbr>`).
fn inline_extras(text: &str, abbreviations: &[(String, String)]) -> Vec<Event<'static>> {
    let mut pieces = vec![Piece::Text(text.to_owned())];
    // Obsidian comments are left out; emoji shortcodes become emoji.
    pieces = replace(pieces, |text| {
        let mut found: Vec<(std::ops::Range<usize>, Option<Piece>)> = Vec::new();
        let mut at = 0;
        while let Some(open) = text[at..].find("%%").map(|ix| ix + at) {
            let Some(close) = text[open + 2..].find("%%").map(|ix| ix + open + 2) else {
                break;
            };
            found.push((open..close + 2, None));
            at = close + 2;
        }
        found
    });
    pieces = replace(pieces, |text| {
        crate::analysis::emoji_shortcodes(text)
            .into_iter()
            .map(|(range, emoji)| (range, Some(Piece::Text(emoji.to_owned()))))
            .collect()
    });
    pieces = split(pieces, |text| {
        let mut found = Vec::new();
        let mut at = 0;
        while let Some(open) = text[at..].find("==").map(|ix| ix + at) {
            let inner = open + 2;
            let Some(close) = text[inner..].find("==").map(|ix| ix + inner) else {
                break;
            };
            if close > inner && !text[inner..close].contains('\n') {
                found.push((open, 2, close, 2, "mark".to_owned()));
                at = close + 2;
            } else {
                at = inner;
            }
        }
        found
    });
    pieces = split(pieces, |text| {
        crate::analysis::superscripts(text)
            .into_iter()
            .map(|(open, close)| (open, 1, close, 1, "sup".to_owned()))
            .collect()
    });
    pieces = split(pieces, |text| {
        crate::analysis::tags(text)
            .into_iter()
            .map(|tag| (tag.start, 0, tag.end, 0, "span class=\"tag\"".to_owned()))
            .collect()
    });
    for (word, title) in abbreviations {
        pieces = split(pieces, |text| {
            let bytes = text.as_bytes();
            let boundary = |at: usize| at >= bytes.len() || !bytes[at].is_ascii_alphanumeric();
            text.match_indices(word.as_str())
                .filter(|(at, _)| (*at == 0 || boundary(at - 1)) && boundary(at + word.len()))
                .map(|(at, _)| {
                    let tag = format!("abbr title=\"{}\"", escape(title));
                    (at, 0, at + word.len(), 0, tag)
                })
                .collect()
        });
    }
    pieces
        .into_iter()
        .map(|piece| match piece {
            Piece::Text(text) => Event::Text(text.into()),
            Piece::Html(html) => Event::InlineHtml(html.into()),
        })
        .collect()
}

/// Replaces the spans `find` reports in each text piece (in order) with a
/// piece, or with nothing.
fn replace(
    pieces: Vec<Piece>,
    find: impl Fn(&str) -> Vec<(std::ops::Range<usize>, Option<Piece>)>,
) -> Vec<Piece> {
    let mut out = Vec::new();
    for piece in pieces {
        let Piece::Text(text) = piece else {
            out.push(piece);
            continue;
        };
        let mut at = 0;
        for (range, replacement) in find(&text) {
            out.push(Piece::Text(text[at..range.start].to_owned()));
            out.extend(replacement);
            at = range.end;
        }
        out.push(Piece::Text(text[at..].to_owned()));
    }
    out
}

/// Splits each text piece where `find` reports spans: (open, its width,
/// close, its width, the tag with attributes) in order, wrapping each
/// span's inside in that tag.
fn split(
    pieces: Vec<Piece>,
    find: impl Fn(&str) -> Vec<(usize, usize, usize, usize, String)>,
) -> Vec<Piece> {
    let mut out = Vec::new();
    for piece in pieces {
        let Piece::Text(text) = piece else {
            out.push(piece);
            continue;
        };
        let mut at = 0;
        for (open, open_width, close, close_width, tag) in find(&text) {
            let name = tag.split_whitespace().next().unwrap_or_default().to_owned();
            out.push(Piece::Text(text[at..open].to_owned()));
            out.push(Piece::Html(format!("<{tag}>")));
            out.push(Piece::Text(text[open + open_width..close].to_owned()));
            out.push(Piece::Html(format!("</{name}>")));
            at = close + close_width;
        }
        out.push(Piece::Text(text[at..].to_owned()));
    }
    out
}

fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            c => escaped.push(c),
        }
    }
    escaped
}

/// A standalone page around `body`, with its styles inline.
pub fn page(title: &str, body: &str) -> String {
    format!(
        "<!doctype html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>{}</title>\n<style>{STYLE}</style>\n</head>\n<body>\n<main>\n{body}</main>\n</body>\n</html>\n",
        escape(title)
    )
}

/// Focal's look for exported pages: its typefaces when installed, its
/// palette, light and dark.
const STYLE: &str = r#"
:root { color-scheme: light dark; --text: #212121; --quiet: #21212199; --background: #f7f6f3;
  --link: #266fb5; --code: #0000000a; --code-text: #864a1c; --mark: #ffd84a66; --rule: #2121211f;
  --note: #2f6fd0; --tip: #2f8f4e; --important: #8250df; --warning: #b0761a; --caution: #c4372f; }
@media (prefers-color-scheme: dark) {
  :root { --text: #dadad7; --quiet: #dadad799; --background: #1a1a1a; --link: #7aaff0;
    --code: #ffffff0d; --code-text: #e0a46e; --mark: #c79a1a66; --rule: #ffffff1f; }
}
body { margin: 0; background: var(--background); color: var(--text);
  font: 18px/1.6 "iA Writer Quattro S", "iA Writer Quattro V", -apple-system, "Helvetica Neue", sans-serif; }
main { max-width: 40em; margin: 0 auto; padding: 3em 1.5em; }
h1, h2, h3, h4, h5, h6 { line-height: 1.3; margin: 1.6em 0 0.5em; }
h1 { font-size: 1.6em; } h2 { font-size: 1.35em; } h3 { font-size: 1.15em; } h4, h5, h6 { font-size: 1em; }
a { color: var(--link); }
code, pre { font-family: "iA Writer Mono S", "iA Writer Mono V", ui-monospace, Menlo, monospace; font-size: 0.86em; }
code { color: var(--code-text); }
pre { background: var(--code); padding: 0.8em 1em; border-radius: 6px; overflow-x: auto; }
pre code { color: inherit; }
mark { background: var(--mark); color: inherit; }
blockquote { margin: 1em 0; padding-left: 1em; border-left: 3px solid var(--rule); color: var(--quiet); }
.callout { margin: 1em 0; padding: 0.1em 0 0.1em 1em; border-left: 3px solid var(--rule); }
.callout-title { font-weight: bold; margin: 0.4em 0; }
.callout-note { border-color: var(--note); } .callout-note > .callout-title { color: var(--note); }
.callout-tip { border-color: var(--tip); } .callout-tip > .callout-title { color: var(--tip); }
.callout-important { border-color: var(--important); } .callout-important > .callout-title { color: var(--important); }
.callout-warning { border-color: var(--warning); } .callout-warning > .callout-title { color: var(--warning); }
.callout-caution { border-color: var(--caution); } .callout-caution > .callout-title { color: var(--caution); }
.callout-quote > .callout-title { color: var(--quiet); }
table { border-collapse: collapse; margin: 1em 0; }
th, td { border: 1px solid var(--rule); padding: 0.3em 0.7em; }
hr { border: none; border-top: 1px solid var(--rule); margin: 2em 0; }
img, figure svg { max-width: 100%; height: auto; }
.math { color: var(--text); } div.math, figure.diagram { text-align: center; margin: 1em 0; }
li:has(> input[type="checkbox"]) { list-style: none; margin-left: -1.3em; }
.footnote-definition { font-size: 0.9em; color: var(--quiet); }
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(text: &str) -> String {
        to_html(text, &mut |_| None)
    }

    #[test]
    fn markdown_becomes_html() {
        let html =
            plain("# Title\n\nSome *emphasis* and ~~gone~~.\n\n| a | b |\n|---|---|\n| 1 | 2 |\n");
        assert!(html.contains(r#"<h1 id="title">Title</h1>"#), "{html}");
        assert!(html.contains("<em>emphasis</em>"), "{html}");
        assert!(html.contains("<del>gone</del>"), "{html}");
        assert!(html.contains("<table>"), "{html}");
    }

    #[test]
    fn front_matter_is_left_out() {
        let html = plain("---\ntitle: Secret\n---\n\nBody\n");
        assert!(!html.contains("Secret"), "{html}");
        assert!(html.contains("<p>Body</p>"), "{html}");
    }

    #[test]
    fn highlights_become_marks_outside_code() {
        let html = plain("A ==marked== word and `==code==`.\n");
        assert!(html.contains("<mark>marked</mark>"), "{html}");
        assert!(html.contains("<code>==code==</code>"), "{html}");
    }

    #[test]
    fn math_and_diagrams_are_rendered_by_the_app() {
        let mut seen = Vec::new();
        let html = to_html(
            "Inline $x^2$.\n\n$$\ny = 1\n$$\n\n```mermaid\nflowchart TD\n  A --> B\n```\n",
            &mut |embed| {
                seen.push(format!("{embed:?}"));
                Some(match embed {
                    Embed::InlineMath(_) => "<svg>inline</svg>".into(),
                    Embed::DisplayMath(_) => "<svg>display</svg>".into(),
                    Embed::Diagram(_) => "<svg>diagram</svg>".into(),
                    Embed::WikiLink(_) | Embed::Image(_) => return None,
                })
            },
        );
        assert!(
            html.contains(r#"<span class="math"><svg>inline</svg></span>"#),
            "{html}"
        );
        assert!(
            html.contains(r#"<div class="math"><svg>display</svg></div>"#),
            "{html}"
        );
        assert!(
            html.contains(r#"<figure class="diagram"><svg>diagram</svg></figure>"#),
            "{html}"
        );
        assert!(seen.iter().any(|s| s.contains("flowchart TD")), "{seen:?}");
    }

    #[test]
    fn unrendered_embeds_show_their_source() {
        let html = plain("$a<b$\n\n```mermaid\nA --> B\n```\n");
        assert!(
            html.contains(r#"<code class="math">a&lt;b</code>"#),
            "{html}"
        );
        assert!(html.contains("A --&gt; B"), "{html}");
    }

    #[test]
    fn wiki_links_point_where_the_app_says_or_beside_the_document() {
        let html = to_html(
            "See [[Other Note]] and [[known]].\n",
            &mut |embed| match embed {
                Embed::WikiLink("known") => Some("notes/known.md".into()),
                _ => None,
            },
        );
        assert!(html.contains(r#"href="Other%20Note.md""#), "{html}");
        assert!(html.contains(r#"href="notes/known.md""#), "{html}");
    }

    #[test]
    fn raw_html_passes_through_as_in_every_markdown_renderer() {
        assert!(plain("<b>raw</b>\n").contains("<b>raw</b>"));
    }

    #[test]
    fn a_page_has_an_escaped_title_and_its_styles() {
        let page = page("A <b> & c", "<p>x</p>");
        assert!(page.starts_with("<!doctype html>"), "{page}");
        assert!(
            page.contains("<title>A &lt;b&gt; &amp; c</title>"),
            "{page}"
        );
        assert!(
            page.contains("<style>") && page.contains("prefers-color-scheme"),
            "{page}"
        );
        assert!(page.contains("<p>x</p>"), "{page}");
    }

    #[test]
    fn image_sources_can_be_rewritten() {
        let html = to_html(
            "![a](pic.png) ![b](https://x/y.png)\n",
            &mut |embed| match embed {
                Embed::Image("pic.png") => Some("file:///notes/pic.png".into()),
                _ => None,
            },
        );
        assert!(html.contains(r#"src="file:///notes/pic.png""#), "{html}");
        assert!(html.contains(r#"src="https://x/y.png""#), "{html}");
    }

    #[test]
    fn every_math_notation_exports_as_math() {
        let mut seen = Vec::new();
        to_html(
            "A \\(x\\) and $`y`$.\n\n\\[\nz\n\\]\n\n```math\nw\n```\n",
            &mut |embed| {
                seen.push(format!("{embed:?}"));
                None
            },
        );
        let joined = seen.join(" ");
        for expected in [
            r#"InlineMath("x")"#,
            r#"InlineMath("y")"#,
            "DisplayMath",
            "z",
            "w",
        ] {
            assert!(joined.contains(expected), "{expected} in {joined}");
        }
    }

    #[test]
    fn callouts_in_every_style_export_as_titled_blocks() {
        for (text, class, title) in [
            (
                "> [!tip] Hello *you*\n> Body **b**\n",
                "callout-tip",
                "✦ Hello <em>you</em>",
            ),
            (
                "> [!WARNING]\n> Body **b**\n",
                "callout-warning",
                "⚠ Warning",
            ),
            (
                "::: danger Hot\nBody **b**\n:::\n",
                "callout-caution",
                "⊘ Hot",
            ),
            (
                "!!! note \"Read\"\n    Body **b**\n",
                "callout-note",
                "ⓘ Read",
            ),
        ] {
            let html = plain(text);
            assert!(
                html.contains(&format!(r#"<div class="callout {class}">"#)),
                "{text:?}: {html}"
            );
            assert!(
                html.contains(&format!(r#"<p class="callout-title">{title}</p>"#)),
                "{text:?}: {html}"
            );
            assert!(html.contains("<strong>b</strong>"), "{text:?}: {html}");
            assert!(
                !html.contains("[!") && !html.contains(":::") && !html.contains("!!!"),
                "{html}"
            );
        }
    }

    #[test]
    fn gitlab_quotes_export_as_quotes() {
        let html = plain("Before\n\n>>>\nQuoted *text*\n>>>\n\nAfter\n");
        assert!(html.contains("<blockquote>"), "{html}");
        assert!(
            html.contains("<em>text</em>") && html.contains("<p>After</p>"),
            "{html}"
        );
    }

    #[test]
    fn pandoc_and_extra_extensions_export() {
        let html = plain(
            "+++\ntitle = \"x\"\n+++\n\n## Title {#custom}\n\nE = mc^2^, 2^10^ and the 1^st^.\n\nTerm\n: Definition\n\nThe HTML spec.\n\n*[HTML]: Hyper Text Markup Language\n",
        );
        assert!(
            !html.contains("title = "),
            "front matter is left out: {html}"
        );
        assert!(html.contains(r#"<h2 id="custom">Title</h2>"#), "{html}");
        for sup in ["<sup>2</sup>", "<sup>10</sup>", "<sup>st</sup>"] {
            assert!(html.contains(sup), "{sup}: {html}");
        }
        assert!(
            html.contains("<dt>Term</dt>") && html.contains("<dd>Definition</dd>"),
            "{html}"
        );
        assert!(
            html.contains(r#"<abbr title="Hyper Text Markup Language">HTML</abbr>"#),
            "{html}"
        );
        assert!(!html.contains("*[HTML]"), "{html}");
    }

    #[test]
    fn obsidian_syntax_and_emoji_export() {
        let html = plain(
            "Hi %%secret%% there :tada: #idea\n\n%%\nblock secret\n%%\n\n![[Some Note]]\n\n![[pic.png|300]]\n",
        );
        assert!(!html.contains("secret"), "comments are left out: {html}");
        assert!(html.contains("🎉") && !html.contains(":tada:"), "{html}");
        assert!(html.contains(r#"<span class="tag">#idea</span>"#), "{html}");
        assert!(
            html.contains(r#"<a href="Some%20Note.md">Some Note</a>"#),
            "{html}"
        );
        assert!(
            html.contains(r#"src="pic.png""#) && html.contains(r#"width="300""#),
            "{html}"
        );
    }

    #[test]
    fn a_table_of_contents_links_to_the_headings() {
        let html = plain("# Guide\n\n[TOC]\n\n## First step\n\n## Second step\n");
        assert!(html.contains(r#"<nav class="toc">"#), "{html}");
        assert!(
            html.contains(r##"<a href="#first-step">First step</a>"##),
            "{html}"
        );
        assert!(
            html.contains(r#"<h2 id="first-step">First step</h2>"#),
            "{html}"
        );
        assert!(!html.contains("[TOC]"), "{html}");
    }
}
