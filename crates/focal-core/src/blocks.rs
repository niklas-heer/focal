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
    for line in 1..end {
        let source = &text[analysis.lines.range(line)];
        let item = source.trim_start();
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
            let value = value.trim();
            let value = value
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
                );
            fields.push((key.trim().to_owned(), value));
        }
    }
    Some(FrontMatter {
        lines: 0..end + 1,
        fields,
    })
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
                destination: "cat%20one.png".into()
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
}
