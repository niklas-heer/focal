//! Drawing a table as a grid: rendered cells, the cell being edited, and the
//! controls around the grid.

use focal_core::analysis::ColumnAlignment;
use focal_core::range_view;
use gpui_kit::component::input::Input;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, CursorStyle, FontWeight, InteractiveElement as _, IntoElement,
    MouseButton, ParentElement as _, Styled as _, StyledText, TestSupportExt as _, canvas, div, px,
    relative,
};

use crate::editor::{Editor, PaintedRow, Row, TEXT_SIZE, text_runs};
use crate::theme::{PROSE_FONT, Theme};

/// Cell text size relative to the body text.
const CELL_SCALE: f32 = 0.92;
const CELL_LINE_HEIGHT: f32 = 1.45;

impl Editor {
    pub(crate) fn render_table(
        &self,
        table_ix: usize,
        theme: &Theme,
        cx: &Context<Self>,
    ) -> AnyElement {
        let table = &self.snapshot.analysis.tables[table_ix];
        let columns = table.rows.iter().map(Vec::len).max().unwrap_or(0);
        let painted = self.painted.clone();
        let rows = (0..table.rows.len()).map(|r| {
            div()
                .id(("row", r))
                .flex()
                .when(r > 0, |d| d.border_t_1())
                .when(r == 0, |d| d.bg(theme.code_background))
                .border_color(theme.rule)
                .children((0..columns).map(|c| self.render_cell(table_ix, r, c, theme, cx)))
        });
        div()
            .id(("table", table_ix))
            .test_support()
            .my(px(6.))
            .text_size(px(TEXT_SIZE * CELL_SCALE))
            .line_height(relative(CELL_LINE_HEIGHT))
            .border_1()
            .border_color(theme.rule)
            .rounded(px(6.))
            .overflow_hidden()
            .children(rows)
            .child(
                canvas(
                    |_, _, _| {},
                    move |bounds, (), _, _| {
                        painted.borrow_mut().push(PaintedRow {
                            row: Row::Table(table_ix),
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
            .relative()
            .into_any_element()
    }

    fn render_cell(
        &self,
        table_ix: usize,
        r: usize,
        c: usize,
        theme: &Theme,
        cx: &Context<Self>,
    ) -> AnyElement {
        let analysis = &self.snapshot.analysis;
        let table = &analysis.tables[table_ix];
        let alignment = table.alignments.get(c).copied();
        let editing = self
            .grid
            .as_ref()
            .filter(|g| g.table == table_ix && g.row == r && g.column == c)
            .map(|g| g.input.clone());
        let content = table.rows[r].get(c).map(|cell| {
            let line = analysis.lines.line_of(cell.start);
            let view = range_view(analysis, self.text(), line, cell.clone(), None);
            let weight = if r == 0 {
                FontWeight::BOLD
            } else {
                FontWeight::NORMAL
            };
            StyledText::new(view.text.clone()).with_runs(text_runs(
                &view.runs, PROSE_FONT, weight, theme.text, theme, None, false,
            ))
        });
        div()
            .id(("cell", c))
            .test_support()
            .flex_1()
            .min_w(px(60.))
            .px(px(10.))
            .py(px(5.))
            .cursor(CursorStyle::IBeam)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    cx.stop_propagation();
                    this.edit_cell(table_ix, r, c, window, cx);
                }),
            )
            .when(c > 0, |d| d.border_l_1())
            .border_color(theme.rule)
            .when(alignment == Some(ColumnAlignment::Center), |d| {
                d.text_center()
            })
            .when(alignment == Some(ColumnAlignment::Right), |d| {
                d.text_right()
            })
            .map(|d| match editing {
                Some(input) => d.child(
                    div().key_context("FocalCell").child(
                        // Match the rendered cell so the row keeps its size
                        // while its cell is edited.
                        Input::new(&input)
                            .appearance(false)
                            .p(px(0.))
                            .h(px(TEXT_SIZE * CELL_SCALE * CELL_LINE_HEIGHT))
                            .text_size(px(TEXT_SIZE * CELL_SCALE))
                            .line_height(relative(CELL_LINE_HEIGHT))
                            .font_family(PROSE_FONT),
                    ),
                ),
                None => d.children(content),
            })
            .into_any_element()
    }
}
