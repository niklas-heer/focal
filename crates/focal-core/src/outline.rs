//! The document's headings, for Go to Heading.

use pulldown_cmark::{Event, Parser, Tag, TagEnd};

/// A heading: its level (1 to 6), its title as plain text, and where that
/// title starts in the source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Heading {
    pub level: u8,
    pub title: String,
    pub offset: usize,
}

/// The headings of `text`, in order.
pub fn outline(text: &str) -> Vec<Heading> {
    let mut headings = Vec::new();
    let mut open: Option<Heading> = None;
    for (event, range) in Parser::new_ext(text, crate::analysis::options()).into_offset_iter() {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                open = Some(Heading {
                    level: level as u8,
                    title: String::new(),
                    offset: range.start,
                });
            }
            Event::End(TagEnd::Heading(_)) => headings.extend(open.take()),
            Event::Text(text) | Event::Code(text) | Event::InlineMath(text) => {
                if let Some(heading) = &mut open {
                    if heading.title.is_empty() {
                        heading.offset = range.start;
                    }
                    heading.title.push_str(&text);
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if let Some(heading) = &mut open {
                    heading.title.push(' ');
                }
            }
            _ => {}
        }
    }
    for heading in &mut headings {
        heading.title = heading.title.trim().to_owned();
    }
    headings
}

#[cfg(test)]
mod tests {
    use super::*;

    fn titles(text: &str) -> Vec<(u8, String)> {
        outline(text)
            .into_iter()
            .map(|h| (h.level, h.title))
            .collect()
    }

    #[test]
    fn atx_headings_with_their_levels() {
        let text = "# One\n\ntext\n\n## Two ##\n\n### Three\n";
        assert_eq!(
            titles(text),
            [(1, "One".into()), (2, "Two".into()), (3, "Three".into())]
        );
        let offsets: Vec<usize> = outline(text).iter().map(|h| h.offset).collect();
        assert_eq!(offsets, [2, 16, 28], "where each title starts");
    }

    #[test]
    fn inline_markup_is_left_out_of_titles() {
        assert_eq!(
            titles("## A **bold** `code` [link](x) title\n"),
            [(2, "A bold code link title".into())]
        );
    }

    #[test]
    fn setext_headings_count() {
        assert_eq!(
            titles("Title\n=====\n\nSub\n---\n"),
            [(1, "Title".into()), (2, "Sub".into())]
        );
    }

    #[test]
    fn hashes_in_code_and_front_matter_are_not_headings() {
        let text = "---\ntitle: x\n---\n\n```sh\n# a comment\n```\n\n# Real\n";
        assert_eq!(titles(text), [(1, "Real".into())]);
    }

    #[test]
    fn no_headings() {
        assert!(outline("").is_empty());
        assert!(outline("just text\n").is_empty());
    }
}
