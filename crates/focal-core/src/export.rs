//! Markdown to HTML, for Export as HTML and Copy as HTML, in the dialect the
//! editor shows. What needs the app (typeset math, Mermaid diagrams, where a
//! wiki link points) comes from a callback.

use pulldown_cmark::{CodeBlockKind, CowStr, Event, LinkType, Parser, Tag, TagEnd};

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
    let mut events: Vec<Event> = Vec::new();
    // A Mermaid block's source while it is read.
    let mut diagram: Option<String> = None;
    let mut in_metadata = false;
    let mut in_code = false;
    let shadowed = crate::shadow::shadow(text);
    for event in Parser::new_ext(&shadowed, crate::analysis::options()) {
        match event {
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
                let html = match render(Embed::Diagram(&source)) {
                    Some(svg) => format!(r#"<figure class="diagram">{svg}</figure>"#),
                    None => format!(
                        r#"<pre><code class="language-mermaid">{}</code></pre>"#,
                        escape(&source)
                    ),
                };
                events.push(Event::Html(html.into()));
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                in_code = true;
                events.push(Event::Start(Tag::CodeBlock(kind)));
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code = false;
                events.push(Event::End(TagEnd::CodeBlock));
            }
            Event::InlineMath(tex) => {
                let html = match render(Embed::InlineMath(&tex)) {
                    Some(svg) => format!(r#"<span class="math">{svg}</span>"#),
                    None => format!(r#"<code class="math">{}</code>"#, escape(&tex)),
                };
                events.push(Event::InlineHtml(html.into()));
            }
            Event::DisplayMath(tex) => {
                let html = match render(Embed::DisplayMath(&tex)) {
                    Some(svg) => format!(r#"<div class="math">{svg}</div>"#),
                    None => format!(r#"<pre class="math">{}</pre>"#, escape(&tex)),
                };
                events.push(Event::Html(html.into()));
            }
            Event::Start(Tag::Link {
                link_type: link_type @ LinkType::WikiLink { .. },
                dest_url,
                title,
                id,
            }) => {
                let href =
                    render(Embed::WikiLink(&dest_url)).unwrap_or_else(|| format!("{dest_url}.md"));
                events.push(Event::Start(Tag::Link {
                    link_type,
                    dest_url: href.into(),
                    title,
                    id,
                }));
            }
            Event::Start(Tag::Image {
                link_type,
                dest_url,
                title,
                id,
            }) => {
                let source = render(Embed::Image(&dest_url)).map_or(dest_url, CowStr::from);
                events.push(Event::Start(Tag::Image {
                    link_type,
                    dest_url: source,
                    title,
                    id,
                }));
            }
            Event::Text(text) if !in_code => push_highlighted(&mut events, text),
            other => events.push(other),
        }
    }
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, events.into_iter());
    html
}

/// `==highlight==` is not part of `pulldown-cmark`; as in the editor, it is
/// found inside one text event.
fn push_highlighted<'a>(events: &mut Vec<Event<'a>>, text: CowStr<'a>) {
    let mut rest: &str = &text;
    let mut parts: Vec<Event<'a>> = Vec::new();
    while let Some(open) = rest.find("==") {
        let inner = &rest[open + 2..];
        let Some(close) = inner.find("==") else {
            break;
        };
        if close == 0 || inner[..close].contains('\n') {
            parts.push(Event::Text(rest[..open + 2].to_owned().into()));
            rest = inner;
            continue;
        }
        parts.push(Event::Text(rest[..open].to_owned().into()));
        parts.push(Event::InlineHtml("<mark>".into()));
        parts.push(Event::Text(inner[..close].to_owned().into()));
        parts.push(Event::InlineHtml("</mark>".into()));
        rest = &inner[close + 2..];
    }
    if parts.is_empty() {
        events.push(Event::Text(text));
    } else {
        parts.push(Event::Text(rest.to_owned().into()));
        events.extend(parts);
    }
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
blockquote[class^="markdown-alert-"] { color: var(--text); }
.markdown-alert-note { border-color: var(--note); } .markdown-alert-tip { border-color: var(--tip); }
.markdown-alert-important { border-color: var(--important); }
.markdown-alert-warning { border-color: var(--warning); } .markdown-alert-caution { border-color: var(--caution); }
blockquote[class^="markdown-alert-"]::before { display: block; font-weight: bold; }
.markdown-alert-note::before { content: "ⓘ Note"; color: var(--note); }
.markdown-alert-tip::before { content: "✦ Tip"; color: var(--tip); }
.markdown-alert-important::before { content: "! Important"; color: var(--important); }
.markdown-alert-warning::before { content: "⚠ Warning"; color: var(--warning); }
.markdown-alert-caution::before { content: "⊘ Caution"; color: var(--caution); }
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
        assert!(html.contains("<h1>Title</h1>"), "{html}");
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
}
