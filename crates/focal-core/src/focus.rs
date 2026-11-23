//! Focus mode: what stays bright around the caret, and whether a drawn block
//! (a table, diagram, image, formula or other island) fades with the text.

use std::ops::Range;

use crate::analysis::{Analysis, LineKind};
use crate::text_stats::sentence_at;

/// The caret's paragraph (as lines) and, by sentence, the sentence in it (as
/// source bytes). Everything else is dimmed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Focus {
    pub lines: Range<usize>,
    pub sentence: Option<Range<usize>>,
}

impl Focus {
    /// What focus mode keeps bright around the caret at `head`: its
    /// paragraph, and with `by_sentence`, the sentence in a paragraph of prose.
    pub fn at(analysis: &Analysis, text: &str, head: usize, by_sentence: bool) -> Self {
        let lines = paragraph_around(analysis, text, head);
        let prose = lines.clone().all(|line| {
            matches!(
                analysis.info(line).kind,
                LineKind::Text | LineKind::Heading(_)
            )
        });
        let sentence = (by_sentence && prose).then(|| {
            // In a list, a sentence never runs past its item.
            let line = analysis.lines.line_of(head);
            let range = if analysis.info(line).prefix.marker.is_some() {
                analysis.content_range(line)
            } else {
                analysis.lines.range(lines.start).start..analysis.lines.range(lines.end - 1).end
            };
            sentence_at(text, range, head)
        });
        Self { lines, sentence }
    }

    /// Whether a block drawn in place of the source `lines` fades, as the text
    /// of those lines would: it shares no line with the caret's paragraph, or
    /// by sentence, no text with the sentence.
    pub fn dims(&self, analysis: &Analysis, lines: &Range<usize>) -> bool {
        if lines.is_empty() || self.lines.end <= lines.start || lines.end <= self.lines.start {
            return true;
        }
        self.sentence.as_ref().is_some_and(|keep| {
            let start = analysis.lines.range(lines.start).start;
            let end = analysis.lines.range(lines.end - 1).end;
            keep.end < start || keep.start > end
        })
    }
}

/// Whether focus mode, if it is on (`focus` is `Some`), dims a block drawn for
/// the source `lines`. Outside focus mode nothing is dimmed.
pub fn dims(focus: Option<&Focus>, analysis: &Analysis, lines: &Range<usize>) -> bool {
    focus.is_some_and(|focus| focus.dims(analysis, lines))
}

/// The lines of the paragraph around `offset`: the run of non-blank lines, or
/// the blank line itself.
pub fn paragraph_around(analysis: &Analysis, text: &str, offset: usize) -> Range<usize> {
    let lines = &analysis.lines;
    let blank = |line: usize| text[lines.range(line)].trim().is_empty();
    let line = lines.line_of(offset);
    if blank(line) {
        return line..line + 1;
    }
    let mut start = line;
    while start > 0 && !blank(start - 1) {
        start -= 1;
    }
    let mut end = line + 1;
    while end < lines.len() && !blank(end) {
        end += 1;
    }
    start..end
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyze;
    use crate::blocks::{block_image, diagram_blocks, math_blocks};

    const DOC: &str = "# Release\n\
        \n\
        We freeze, stage and release.\n\
        \n\
        - [ ] Freeze\n\
        - [x] Stage\n\
        \n\
        ```mermaid\n\
        flowchart LR\n\
        Freeze --> Staging --> Release\n\
        ```\n\
        \n\
        ![Plan](plan.png)\n\
        \n\
        $$\n\
        x^2\n\
        $$\n";

    fn caret_in(text: &str, needle: &str) -> usize {
        text.find(needle).unwrap()
    }

    #[test]
    fn blocks_outside_the_caret_paragraph_fade_in_focus_mode() {
        let analysis = analyze(DOC);
        let diagram = diagram_blocks(&analysis, DOC).remove(0).lines;
        let math = math_blocks(&analysis, DOC).remove(0).lines;
        let image_line = analysis.lines.line_of(caret_in(DOC, "![Plan]"));
        assert!(block_image(&analysis, DOC, image_line).is_some());
        let image = image_line..image_line + 1;

        let head = caret_in(DOC, "stage and");
        for by_sentence in [false, true] {
            let focus = Focus::at(&analysis, DOC, head, by_sentence);
            for block in [&diagram, &image, &math] {
                assert!(dims(Some(&focus), &analysis, block), "{block:?}");
                assert!(!dims(None, &analysis, block), "{block:?} without focus");
            }
        }
    }

    #[test]
    fn a_block_the_caret_is_in_stays_bright() {
        let analysis = analyze(DOC);
        let diagram = diagram_blocks(&analysis, DOC).remove(0).lines;
        let focus = Focus::at(&analysis, DOC, caret_in(DOC, "Staging"), true);
        assert_eq!(focus.sentence, None, "a diagram is not prose");
        assert!(!focus.dims(&analysis, &diagram));
    }

    #[test]
    fn by_sentence_an_image_in_the_paragraph_fades_outside_the_sentence() {
        let text = "One sentence. Two sentence.\n![Plan](plan.png)\n";
        let analysis = analyze(text);
        let image = 1..2;
        let by_paragraph = Focus::at(&analysis, text, 2, false);
        assert!(
            !by_paragraph.dims(&analysis, &image),
            "it is in the paragraph"
        );
        let by_sentence = Focus::at(&analysis, text, 2, true);
        assert_eq!(
            &text[by_sentence.sentence.clone().unwrap()],
            "One sentence."
        );
        assert!(by_sentence.dims(&analysis, &image));
    }

    #[test]
    fn the_paragraph_is_the_run_of_non_blank_lines() {
        let analysis = analyze(DOC);
        let head = caret_in(DOC, "Stage");
        assert_eq!(paragraph_around(&analysis, DOC, head), 4..6);
        assert_eq!(
            paragraph_around(&analysis, DOC, caret_in(DOC, "\n\nWe") + 1),
            1..2
        );
    }
}
