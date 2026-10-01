//! Islands: blocks drawn in place of their source lines while the caret is
//! elsewhere (front matter, and later images and display math). A click or
//! the caret moving in shows the source.

use focal_core::Analysis;
use focal_core::blocks::front_matter;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, Styled as _,
    TestSupportExt as _, canvas, div, px,
};

use crate::editor::{Editor, Island, IslandKind, PaintedRow, Row};
use crate::theme::Theme;

/// The islands of this version of the text.
pub(crate) fn islands(analysis: &Analysis, text: &str) -> Vec<Island> {
    let mut islands = Vec::new();
    if let Some(matter) = front_matter(analysis, text) {
        islands.push(Island {
            kind: IslandKind::FrontMatter,
            start: matter.lines.start,
            end: matter.lines.end,
        });
    }
    islands
}

impl Editor {
    /// Where the caret goes when an island is clicked: front matter's first
    /// field, otherwise the island's first line.
    pub(crate) fn island_entry(&self, island: Island) -> usize {
        let line = match island.kind {
            IslandKind::FrontMatter if island.end - island.start > 2 => island.start + 1,
            IslandKind::FrontMatter => island.start,
        };
        self.snapshot.analysis.lines.range(line).start
    }

    /// Where a newly opened document puts the caret: at its body, after any
    /// front matter.
    pub(crate) fn body_start(&self) -> usize {
        let analysis = &self.snapshot.analysis;
        let Some(matter) = front_matter(analysis, self.text()) else {
            return 0;
        };
        (matter.lines.end..analysis.line_count())
            .map(|line| analysis.lines.range(line))
            .find(|range| !self.text()[range.clone()].trim().is_empty())
            .map_or(self.text().len(), |range| range.start)
    }

    pub(crate) fn render_island(
        &self,
        island: Island,
        theme: &Theme,
        cx: &Context<Self>,
    ) -> AnyElement {
        let _ = cx;
        let painted = self.painted.clone();
        let content = match island.kind {
            IslandKind::FrontMatter => self.render_front_matter(theme),
        };
        div()
            .relative()
            .child(content)
            .child(
                canvas(
                    |_, _, _| {},
                    move |bounds, (), _, _| {
                        painted.borrow_mut().push(PaintedRow {
                            row: Row::Island(island),
                            layout: None,
                            view: None,
                            bounds,
                        });
                    },
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .into_any_element()
    }

    /// Front matter as one quiet line of keys and values.
    fn render_front_matter(&self, theme: &Theme) -> AnyElement {
        let fields = front_matter(&self.snapshot.analysis, self.text())
            .map(|matter| matter.fields)
            .unwrap_or_default();
        let size = self.typography.size;
        div()
            .id("front-matter")
            .test_support()
            .py(px(6.))
            .flex()
            .flex_wrap()
            .gap_x(px(18.))
            .gap_y(px(4.))
            .text_size(px(size * 0.78))
            .cursor_pointer()
            .when(fields.is_empty(), |d| {
                d.child(div().text_color(theme.marker).child("Front matter"))
            })
            .children(fields.into_iter().map(|(key, value)| {
                div()
                    .flex()
                    .gap(px(6.))
                    .child(div().text_color(theme.marker).child(key))
                    .child(div().text_color(theme.text.opacity(0.75)).child(value))
            }))
            .into_any_element()
    }
}
