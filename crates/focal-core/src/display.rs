//! Turns a source line into the text Focal displays: styled runs with hidden
//! or replaced markers, plus a map between display and source offsets.

use std::ops::Range;

use crate::analysis::{Analysis, InlineStyle, Marker, Piece, Replacement, Reveal};

/// The caret and selection, used to decide which markers are revealed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Caret {
    pub selection: Range<usize>,
    pub head: usize,
    /// The lines the selection touches.
    pub lines: Range<usize>,
}

impl Caret {
    pub fn new(analysis: &Analysis, selection: Range<usize>, head: usize) -> Self {
        let lines = analysis.lines.lines_of(&selection);
        Self {
            selection,
            head,
            lines,
        }
    }

    fn reveals(&self, marker: &Marker) -> bool {
        let sel = &self.selection;
        match &marker.reveal {
            Reveal::Touching(range) => sel.start <= range.end && sel.end >= range.start,
            Reveal::Lines(lines) => self.lines.start < lines.end && lines.start < self.lines.end,
            Reveal::Inside => {
                let range = &marker.range;
                if sel.is_empty() {
                    range.start <= self.head && self.head < range.end
                } else {
                    sel.start < range.end && range.start < sel.end
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SegmentKind {
    Text,
    Hidden,
    Replaced,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Segment {
    pub source: Range<usize>,
    pub display: Range<usize>,
    pub kind: SegmentKind,
    /// The marker this segment hides or replaces.
    pub marker: Option<usize>,
}

/// Maps between absolute source offsets and offsets in a line's display text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DisplayMap {
    pub segments: Vec<Segment>,
}

impl DisplayMap {
    pub fn source_range(&self) -> Range<usize> {
        match (self.segments.first(), self.segments.last()) {
            (Some(first), Some(last)) => first.source.start..last.source.end,
            _ => 0..0,
        }
    }

    /// The display offset for a source offset. Offsets inside a hidden marker
    /// map to where the marker would be.
    pub fn to_display(&self, source: usize) -> usize {
        for segment in &self.segments {
            let inside = segment.source.start <= source && source <= segment.source.end;
            match segment.kind {
                SegmentKind::Text if inside => {
                    return segment.display.start + (source - segment.source.start);
                }
                SegmentKind::Hidden if inside && source < segment.source.end => {
                    return segment.display.start;
                }
                SegmentKind::Replaced if inside && source < segment.source.end => {
                    return if source == segment.source.start {
                        segment.display.start
                    } else {
                        segment.display.end
                    };
                }
                _ => {}
            }
        }
        self.segments
            .last()
            .map_or(0, |segment| segment.display.end)
    }

    /// The source offset for a display offset. Text segments win at shared
    /// boundaries, so a click right after hidden `**` lands inside the span.
    pub fn to_source(&self, display: usize) -> usize {
        for segment in &self.segments {
            if segment.kind == SegmentKind::Text
                && segment.display.start <= display
                && display <= segment.display.end
            {
                return segment.source.start + (display - segment.display.start);
            }
        }
        for segment in &self.segments {
            if segment.kind == SegmentKind::Replaced
                && segment.display.start <= display
                && display <= segment.display.end
            {
                return segment.source.end;
            }
        }
        self.source_range().end
    }

    /// The replaced segment drawn at a display offset, such as a task box.
    pub fn replaced_at(&self, display: usize) -> Option<&Segment> {
        self.segments.iter().find(|segment| {
            segment.kind == SegmentKind::Replaced
                && segment.display.start <= display
                && display < segment.display.end
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Run {
    pub len: usize,
    pub style: InlineStyle,
}

/// One line as displayed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LineView {
    pub text: String,
    pub runs: Vec<Run>,
    pub map: DisplayMap,
}

/// Builds the display of `line`. With no caret, every marker stays hidden.
pub fn line_view(analysis: &Analysis, text: &str, line: usize, caret: Option<&Caret>) -> LineView {
    range_view(analysis, text, line, analysis.lines.range(line), caret)
}

/// Builds the display of part of `line`, such as one table cell.
pub fn range_view(
    analysis: &Analysis,
    text: &str,
    line: usize,
    range: Range<usize>,
    caret: Option<&Caret>,
) -> LineView {
    let within = |piece: &&Piece| range.start <= piece.range.start && piece.range.end <= range.end;
    let markers = analysis
        .line_markers
        .get(line)
        .map_or(&[][..], Vec::as_slice);
    let styles = analysis
        .line_styles
        .get(line)
        .map_or(&[][..], Vec::as_slice);

    let mut hidden: Vec<(Range<usize>, usize, Option<&Replacement>)> = Vec::new();
    let mut revealed: Vec<Range<usize>> = Vec::new();
    for piece in markers.iter().filter(within) {
        let marker = &analysis.markers[piece.index];
        if caret.is_some_and(|caret| caret.reveals(marker)) {
            revealed.push(piece.range.clone());
        } else {
            let replacement = marker.replacement.as_ref().filter(|_| piece.first);
            hidden.push((piece.range.clone(), piece.index, replacement));
        }
    }
    hidden.sort_by_key(|(range, ..)| (range.start, std::cmp::Reverse(range.end)));
    // Merge overlapping markers; the outermost one keeps its replacement.
    let mut merged: Vec<(Range<usize>, usize, Option<&Replacement>)> = Vec::new();
    for (range, index, replacement) in hidden {
        match merged.last_mut() {
            Some((last, ..)) if range.start < last.end => last.end = last.end.max(range.end),
            _ => merged.push((range, index, replacement)),
        }
    }

    let spans: Vec<(Range<usize>, InlineStyle)> = styles
        .iter()
        .filter(|piece| piece.range.start < range.end && range.start < piece.range.end)
        .map(|piece| (piece.range.clone(), analysis.styles[piece.index].style))
        .chain(revealed.into_iter().map(|r| (r, InlineStyle::MARKER)))
        .collect();

    let mut view = LineView::default();
    let mut pos = range.start;
    for (marker, index, replacement) in merged {
        if marker.start > pos {
            push_text(&mut view, text, pos..marker.start, &spans);
        }
        let display_start = view.text.len();
        match replacement {
            Some(replacement) => {
                let shown = replacement.text();
                view.text.push_str(&shown);
                push_run(&mut view.runs, shown.len(), replacement.style());
                view.map.segments.push(Segment {
                    source: marker.clone(),
                    display: display_start..view.text.len(),
                    kind: SegmentKind::Replaced,
                    marker: Some(index),
                });
            }
            None => view.map.segments.push(Segment {
                source: marker.clone(),
                display: display_start..display_start,
                kind: SegmentKind::Hidden,
                marker: Some(index),
            }),
        }
        pos = pos.max(marker.end);
    }
    if pos < range.end || view.map.segments.is_empty() {
        push_text(&mut view, text, pos..range.end, &spans);
    }
    view
}

/// Display ranges of plain prose, where spelling is checked: everything except
/// code, math, HTML, revealed markers and replacements such as bullets.
pub fn prose_ranges(view: &LineView) -> Vec<Range<usize>> {
    let excluded = InlineStyle::CODE
        | InlineStyle::MATH
        | InlineStyle::HTML
        | InlineStyle::MARKER
        | InlineStyle::LABEL
        | InlineStyle::FOOTNOTE;
    let replaced: Vec<Range<usize>> = view
        .map
        .segments
        .iter()
        .filter(|segment| segment.kind == SegmentKind::Replaced)
        .map(|segment| segment.display.clone())
        .collect();
    let mut ranges: Vec<Range<usize>> = Vec::new();
    let mut start = 0;
    for run in &view.runs {
        let range = start..start + run.len;
        start = range.end;
        let is_replaced = replaced
            .iter()
            .any(|r| r.start <= range.start && range.end <= r.end);
        if run.style.0 & excluded.0 != 0 || is_replaced {
            continue;
        }
        match ranges.last_mut() {
            Some(last) if last.end == range.start => last.end = range.end,
            _ => ranges.push(range),
        }
    }
    ranges
}

/// Adds `style` to the parts of `runs` inside `ranges` (display offsets),
/// splitting runs at range edges.
pub fn mark_runs(runs: &[Run], ranges: &[Range<usize>], style: InlineStyle) -> Vec<Run> {
    let mut marked = Vec::new();
    let mut start = 0;
    for run in runs {
        let end = start + run.len;
        let mut cuts = vec![start, end];
        for range in ranges {
            for point in [range.start, range.end] {
                if start < point && point < end {
                    cuts.push(point);
                }
            }
        }
        cuts.sort_unstable();
        cuts.dedup();
        for pair in cuts.windows(2) {
            let inside = ranges
                .iter()
                .any(|r| r.start <= pair[0] && pair[1] <= r.end);
            let piece_style = if inside { run.style | style } else { run.style };
            push_run(&mut marked, pair[1] - pair[0], piece_style);
        }
        start = end;
    }
    marked
}

fn push_text(
    view: &mut LineView,
    text: &str,
    range: Range<usize>,
    spans: &[(Range<usize>, InlineStyle)],
) {
    let display_start = view.text.len();
    view.text.push_str(&text[range.clone()]);
    let mut bounds = vec![range.start, range.end];
    for (span, _) in spans {
        for point in [span.start, span.end] {
            if range.start < point && point < range.end {
                bounds.push(point);
            }
        }
    }
    bounds.sort_unstable();
    bounds.dedup();
    for window in bounds.windows(2) {
        let (start, end) = (window[0], window[1]);
        let style = spans
            .iter()
            .filter(|(span, _)| span.start <= start && end <= span.end)
            .fold(InlineStyle::NONE, |acc, (_, style)| acc | *style);
        push_run(&mut view.runs, end - start, style);
    }
    view.map.segments.push(Segment {
        source: range,
        display: display_start..view.text.len(),
        kind: SegmentKind::Text,
        marker: None,
    });
}

fn push_run(runs: &mut Vec<Run>, len: usize, style: InlineStyle) {
    if len == 0 {
        return;
    }
    match runs.last_mut() {
        Some(last) if last.style == style => last.len += len,
        _ => runs.push(Run { len, style }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::analyze;

    fn view(text: &str, line: usize, caret: Option<usize>) -> LineView {
        let analysis = analyze(text);
        let caret = caret.map(|offset| Caret::new(&analysis, offset..offset, offset));
        line_view(&analysis, text, line, caret.as_ref())
    }

    #[test]
    fn hides_markers_away_from_the_caret() {
        let text = "a **bold** b";
        assert_eq!(view(text, 0, Some(0)).text, "a bold b");
        assert_eq!(view(text, 0, Some(12)).text, "a bold b");
    }

    #[test]
    fn reveals_markers_the_caret_touches() {
        let text = "a **bold** b";
        let revealed = view(text, 0, Some(2));
        assert_eq!(revealed.text, "a **bold** b");
        assert_eq!(
            revealed.runs[1],
            Run {
                len: 2,
                style: InlineStyle::MARKER
            }
        );
    }

    #[test]
    fn maps_offsets_across_hidden_markers() {
        let text = "a **bold** b";
        let map = view(text, 0, None).map;
        assert_eq!(map.to_display(4), 2);
        assert_eq!(map.to_display(8), 6);
        assert_eq!(map.to_display(12), 8);
        // A click right after "bold" lands inside the span, not after `**`.
        assert_eq!(map.to_source(6), 8);
        assert_eq!(map.to_source(2), 2);
    }

    #[test]
    fn heading_reveals_on_its_line() {
        let text = "# Title\nnext";
        assert_eq!(view(text, 0, Some(10)).text, "Title");
        assert_eq!(view(text, 0, Some(3)).text, "# Title");
    }

    #[test]
    fn bullets_are_replaced_until_the_caret_enters() {
        let text = "- item";
        let away = view(text, 0, Some(6));
        assert_eq!(away.text, "•\u{2002}item");
        assert_eq!(away.map.to_source(0), 2);
        assert!(away.map.replaced_at(0).is_some());
        assert_eq!(view(text, 0, Some(1)).text, "- item");
    }

    #[test]
    fn empty_line_has_one_segment() {
        let text = "a\n\nb";
        let empty = view(text, 1, None);
        assert_eq!(empty.text, "");
        assert_eq!(empty.map.to_source(0), 2);
        assert_eq!(empty.map.to_display(2), 0);
    }

    #[test]
    fn crlf_is_never_displayed() {
        let text = "**a**\r\nb";
        assert_eq!(view(text, 0, None).text, "a");
        assert_eq!(view(text, 1, None).text, "b");
    }

    #[test]
    fn range_view_renders_one_table_cell() {
        let text = "| a | **b** |\n|---|---|\n";
        let analysis = analyze(text);
        let cell = analysis.tables[0].rows[0][1].clone();
        assert_eq!(range_view(&analysis, text, 0, cell, None).text, "b");
    }

    #[test]
    fn prose_excludes_code_and_revealed_markers() {
        let text = "See `x_y` and **bold** here";
        let analysis = analyze(text);
        let caret = Caret::new(&analysis, 16..16, 16);
        let view = line_view(&analysis, text, 0, Some(&caret));
        let prose: Vec<&str> = prose_ranges(&view)
            .into_iter()
            .map(|r| &view.text[r])
            .collect();
        assert_eq!(prose, ["See ", " and ", "bold", " here"]);
    }

    #[test]
    #[allow(clippy::single_range_in_vec_init)] // one misspelled word
    fn marks_split_runs_at_range_edges() {
        let runs = [
            Run {
                len: 4,
                style: InlineStyle::NONE,
            },
            Run {
                len: 6,
                style: InlineStyle::STRONG,
            },
        ];
        let marked = mark_runs(&runs, &[2..6], InlineStyle::MISSPELLED);
        assert_eq!(
            marked,
            [
                Run {
                    len: 2,
                    style: InlineStyle::NONE
                },
                Run {
                    len: 2,
                    style: InlineStyle::MISSPELLED
                },
                Run {
                    len: 2,
                    style: InlineStyle::STRONG | InlineStyle::MISSPELLED
                },
                Run {
                    len: 4,
                    style: InlineStyle::STRONG
                },
            ]
        );
    }

    #[test]
    fn nested_emphasis() {
        let text = "***both***";
        let hidden = view(text, 0, None);
        assert_eq!(hidden.text, "both");
        assert_eq!(
            hidden.runs[0].style,
            InlineStyle::STRONG | InlineStyle::EMPHASIS
        );
    }
}
