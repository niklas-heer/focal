//! Blocks Focal draws in place of their source while the caret is
//! elsewhere: front matter, images on their own line and display math.

use std::ops::Range;

use crate::analysis::{Analysis, LineKind};

/// YAML front matter at the top of a document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrontMatter {
    /// Its lines, fences included.
    pub lines: Range<usize>,
    /// Top-level `key: value` pairs; list items join with commas.
    pub fields: Vec<(String, String)>,
}

/// The document's front matter, if it starts with one.
pub fn front_matter(analysis: &Analysis, text: &str) -> Option<FrontMatter> {
    if analysis.line_count() == 0 || analysis.info(0).kind != LineKind::FrontMatterFence {
        return None;
    }
    let end = (1..analysis.line_count())
        .find(|&line| analysis.info(line).kind == LineKind::FrontMatterFence)?;
    let mut fields: Vec<(String, String)> = Vec::new();
    let fence = text[analysis.lines.range(0)].trim();
    for line in 1..end {
        let source = &text[analysis.lines.range(line)];
        let item = source.trim_start();
        // JSON (GitLab's `;;;`): `"key": value,` lines inside braces.
        if fence == ";;;" {
            let item = item.trim_end().trim_end_matches(',');
            if let Some((key, value)) = item.split_once("\":") {
                let key = key.trim_start_matches(['{', ' ']).trim_matches('"');
                let value = value.trim().trim_end_matches('}').trim();
                fields.push((key.to_owned(), unquote(value)));
            }
            continue;
        }
        // TOML (`+++`): `key = value`.
        if fence == "+++" {
            if let Some((key, value)) = item.split_once('=') {
                fields.push((key.trim().to_owned(), list_or_value(value.trim())));
            }
            continue;
        }
        let indented = item.len() < source.len();
        if indented {
            // A list item adds to the last key; other nested lines are skipped.
            if let (Some(value), Some((_, joined))) = (item.strip_prefix("- "), fields.last_mut()) {
                if !joined.is_empty() {
                    joined.push_str(", ");
                }
                joined.push_str(&unquote(value.trim()));
            }
            continue;
        }
        if let Some((key, value)) = item.split_once(':') {
            fields.push((key.trim().to_owned(), list_or_value(value.trim())));
        }
    }
    Some(FrontMatter {
        lines: 0..end + 1,
        fields,
    })
}

/// A `[a, b]` list joined with commas, or the value unquoted.
fn list_or_value(value: &str) -> String {
    value
        .strip_prefix('[')
        .and_then(|v| v.strip_suffix(']'))
        .map_or_else(
            || unquote(value),
            |list| {
                list.split(',')
                    .map(|v| unquote(v.trim()))
                    .collect::<Vec<_>>()
                    .join(", ")
            },
        )
}

fn unquote(value: &str) -> String {
    let quoted = value.len() >= 2
        && ((value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\'')));
    if quoted {
        value[1..value.len() - 1].to_owned()
    } else {
        value.to_owned()
    }
}

/// An image alone on its line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageRef {
    pub alt: String,
    pub destination: String,
    /// An Obsidian embed, found anywhere in the folder.
    pub wiki: bool,
    /// The width asked for, in points.
    pub width: Option<u32>,
    /// Inside a centered HTML wrapper, `<p align="center">` and the like.
    pub centered: bool,
}

/// The image on `line` if nothing else is (beyond quote or list markers).
pub fn block_image(analysis: &Analysis, text: &str, line: usize) -> Option<ImageRef> {
    let content = analysis.content_range(line);
    let source = &text[content.clone()];
    let start = content.start + (source.len() - source.trim_start().len());
    let end = start + source.trim().len();
    analysis
        .images
        .iter()
        .find(|image| image.range == (start..end))
        .map(|image| ImageRef {
            alt: image.alt.clone(),
            destination: image.destination.clone(),
            wiki: image.wiki,
            width: image.width,
            centered: line > 0 && {
                let above = text[analysis.lines.range(line - 1)]
                    .trim()
                    .to_ascii_lowercase();
                above == "<center>"
                    || ((above.starts_with("<p ") || above.starts_with("<div "))
                        && above.contains("align=\"center\""))
            },
        })
}

/// Display math (`$$ … $$`) that stands alone in its lines.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MathBlock {
    pub lines: Range<usize>,
    /// The TeX between the dollar signs.
    pub tex: String,
}

/// The display math that fills whole lines (beyond quote or list markers).
pub fn math_blocks(analysis: &Analysis, text: &str) -> Vec<MathBlock> {
    analysis
        .display_math
        .iter()
        .filter_map(|(range, tex)| {
            let lines = analysis.lines.lines_of(range);
            let first = analysis.content_range(lines.start);
            let last = analysis.lines.range(lines.end - 1);
            let before = &text[first.start..range.start];
            let after = &text[range.end..last.end];
            (before.trim().is_empty() && after.trim().is_empty()).then(|| MathBlock {
                lines,
                tex: tex.trim().to_owned(),
            })
        })
        .collect()
}

/// A fenced `mermaid` code block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiagramBlock {
    /// Its lines, fences included.
    pub lines: Range<usize>,
    /// The diagram's source between the fences.
    pub source: String,
}

/// The fenced code blocks whose language is `mermaid`, in any case.
pub fn diagram_blocks(analysis: &Analysis, text: &str) -> Vec<DiagramBlock> {
    analysis
        .code_blocks
        .iter()
        .filter(|block| {
            block
                .language
                .as_deref()
                .is_some_and(|language| language.eq_ignore_ascii_case("mermaid"))
        })
        .map(|block| {
            let lines = block.lines.clone();
            let closed = lines.len() > 1
                && analysis.info(lines.end - 1).kind == (LineKind::CodeFence { opening: false });
            let inner = lines.start + 1..if closed { lines.end - 1 } else { lines.end };
            let source = inner
                .clone()
                .map(|line| &text[analysis.lines.range(line)])
                .collect::<Vec<_>>()
                .join("\n");
            DiagramBlock {
                lines,
                source: source.trim_end().to_owned(),
            }
        })
        .collect()
}

/// The lines of each table-of-contents marker: `[TOC]` (Markdown Extra,
/// GitLab), `[[_TOC_]]` (GitLab), `[[toc]]` (VitePress), and kramdown's
/// `{:toc}` with its `* TOC` line above.
pub fn toc_blocks(analysis: &Analysis, text: &str) -> Vec<Range<usize>> {
    let mut found = Vec::new();
    for line in 0..analysis.line_count() {
        if !matches!(analysis.info(line).kind, LineKind::Text) {
            continue;
        }
        let content = text[analysis.lines.range(line)].trim();
        let marker = matches!(
            content.to_ascii_lowercase().as_str(),
            "[toc]" | "[[_toc_]]" | "[[toc]]" | "{:toc}"
        );
        if !marker {
            continue;
        }
        let kramdown_list = content == "{:toc}"
            && line > 0
            && matches!(
                text[analysis.lines.range(line - 1)].trim(),
                "* TOC" | "- TOC" | "1. TOC"
            );
        found.push(if kramdown_list { line - 1 } else { line }..line + 1);
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::analyze;

    #[test]
    fn an_image_alone_on_its_line_is_a_block() {
        let text = "![A cat](cat%20one.png \"title\")\ntext ![b](b.png)\n[![c](c.png)](https://x)\n> ![d](https://e.com/d.jpg)\n";
        let analysis = analyze(text);
        assert_eq!(
            block_image(&analysis, text, 0),
            Some(ImageRef {
                alt: "A cat".into(),
                destination: "cat%20one.png".into(),
                wiki: false,
                width: None,
                centered: false,
            })
        );
        assert_eq!(block_image(&analysis, text, 1), None, "inside text");
        assert_eq!(block_image(&analysis, text, 2), None, "inside a link");
        assert_eq!(
            block_image(&analysis, text, 3).map(|i| i.destination),
            Some("https://e.com/d.jpg".into()),
            "inside a quote"
        );
    }

    #[test]
    fn display_math_alone_on_its_lines_is_a_block() {
        let text = "$$\n\\frac{a}{b}\n$$\n\nInline $$x$$ here.\n\n$$E = mc^2$$\n";
        let blocks = math_blocks(&analyze(text), text);
        assert_eq!(
            blocks,
            [
                MathBlock {
                    lines: 0..3,
                    tex: "\\frac{a}{b}".into()
                },
                MathBlock {
                    lines: 6..7,
                    tex: "E = mc^2".into()
                },
            ]
        );
    }

    #[test]
    fn mermaid_code_blocks_are_diagrams() {
        let text = "```mermaid\nflowchart TD\n  A --> B\n```\n\n```rust\nfn x() {}\n```\n\n```Mermaid\npie\n";
        let blocks = diagram_blocks(&analyze(text), text);
        assert_eq!(
            blocks,
            [
                DiagramBlock {
                    lines: 0..4,
                    source: "flowchart TD\n  A --> B".into()
                },
                DiagramBlock {
                    lines: 9..11,
                    source: "pie".into()
                },
            ]
        );
    }

    #[test]
    fn front_matter_fields_are_read_plainly() {
        let text = "---\ntitle: \"Focal: a showcase\"\ntags: [markdown, spike]\nauthors:\n  - Ann\n  - Bo\nnested:\n  key: skipped\n---\n\nBody\n";
        let matter = front_matter(&analyze(text), text).unwrap();
        assert_eq!(matter.lines, 0..9);
        assert_eq!(
            matter.fields,
            [
                ("title".to_owned(), "Focal: a showcase".to_owned()),
                ("tags".to_owned(), "markdown, spike".to_owned()),
                ("authors".to_owned(), "Ann, Bo".to_owned()),
                ("nested".to_owned(), String::new()),
            ]
        );
        assert!(front_matter(&analyze("# No matter\n"), "# No matter\n").is_none());
    }

    #[test]
    fn toml_and_json_front_matter_fields() {
        let text = "+++\ntitle = \"Hi\"\ntags = [\"a\", \"b\"]\n+++\n";
        let fields = front_matter(&analyze(text), text).unwrap().fields;
        assert_eq!(
            fields,
            [
                ("title".into(), "Hi".into()),
                ("tags".into(), "a, b".into())
            ]
        );
        let text = ";;;\n{\n  \"title\": \"Hi\",\n  \"draft\": false\n}\n;;;\n";
        let fields = front_matter(&analyze(text), text).unwrap().fields;
        assert_eq!(
            fields,
            [
                ("title".into(), "Hi".into()),
                ("draft".into(), "false".into())
            ]
        );
    }

    #[test]
    fn table_of_contents_markers() {
        let text = "[TOC]\n\n[[_TOC_]]\n\n[[toc]]\n\n* TOC\n{:toc}\n\n```\n[TOC]\n```\n";
        let found = toc_blocks(&analyze(text), text);
        assert_eq!(found, [0..1, 2..3, 4..5, 6..8]);
    }

    #[test]
    fn images_in_a_centered_wrapper_are_centered() {
        let text = "<p align=\"center\">\n  <img src=\"logo.png\">\n</p>\n\n![a](b.png)\n";
        let analysis = analyze(text);
        assert!(block_image(&analysis, text, 1).unwrap().centered);
        assert!(!block_image(&analysis, text, 4).unwrap().centered);
    }
}
