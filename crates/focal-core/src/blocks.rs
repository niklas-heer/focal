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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::analyze;

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
