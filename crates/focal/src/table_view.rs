//! Drawing a table as a grid: rendered cells, the cell being edited, and the
//! controls around the grid.

use focal_core::analysis::ColumnAlignment;
use focal_core::{TableModel, range_view};
use gpui_kit::component::input::Input;
use gpui_kit::component::native_menu::NativeMenu;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, CursorStyle, FontWeight, InteractiveElement as _, IntoElement,
    MouseButton, MouseDownEvent, ParentElement as _, Styled as _, StyledText, TestSupportExt as _,
    Window, canvas, div, px, relative,
};

use crate::editor::{Editor, PaintedRow, Row, TEXT_SIZE, text_runs};
use crate::grid::{TableOp, TableOpKind};
use crate::theme::{PROSE_FONT, Theme};

/// Hover group of a table, for its add-row and add-column buttons.
const TABLE_GROUP: &str = "focal-table";
/// Room around the grid for the hover buttons.
const HOVER_BUTTON: f32 = 22.;

/// A `+` button beside the grid that shows while the pointer is over the table.
fn hover_button(
    id: &'static str,
    theme: &Theme,
    cx: &Context<Editor>,
    table: usize,
    row: usize,
    column: usize,
    op: TableOpKind,
) -> gpui_kit::Stateful<gpui_kit::Div> {
    div()
        .id(id)
        .absolute()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(4.))
        .text_color(theme.marker)
        .opacity(0.)
        .group_hover(TABLE_GROUP, |style| style.opacity(1.))
        .hover(|style| style.bg(theme.code_background))
        .cursor_pointer()
        .child("+")
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _, window, cx| {
                cx.stop_propagation();
                this.table_op(
                    &TableOp {
                        table,
                        row,
                        column,
                        op,
                    },
                    window,
                    cx,
                );
            }),
        )
}

/// Cell text size relative to the body text.
const CELL_SCALE: f32 = 0.92;
const CELL_LINE_HEIGHT: f32 = 1.45;

impl Editor {
    fn show_cell_menu(
        &mut self,
        table: usize,
        row: usize,
        column: usize,
        position: gpui_kit::Point<gpui_kit::Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use TableOpKind::*;
        let Some(model) = TableModel::from_analysis(&self.snapshot.analysis, self.text(), table)
        else {
            return;
        };
        let op = |kind| {
            Box::new(TableOp {
                table,
                row,
                column,
                op: kind,
            })
        };
        let align = NativeMenu::new()
            .menu("Left", op(AlignLeft))
            .menu("Center", op(AlignCenter))
            .menu("Right", op(AlignRight))
            .menu("None", op(AlignNone));
        NativeMenu::new()
            .menu("Insert Row Above", op(InsertRowAbove))
            .menu("Insert Row Below", op(InsertRowBelow))
            .menu_with_disabled("Delete Row", row == 0, op(DeleteRow))
            .separator()
            .menu("Insert Column Left", op(InsertColumnLeft))
            .menu("Insert Column Right", op(InsertColumnRight))
            .menu_with_disabled("Delete Column", model.column_count() < 2, op(DeleteColumn))
            .separator()
            .submenu("Align Column", align)
            .separator()
            .menu_with_disabled("Move Row Up", row < 2, op(MoveRowUp))
            .menu_with_disabled(
                "Move Row Down",
                row == 0 || row + 1 >= model.rows.len(),
                op(MoveRowDown),
            )
            .menu_with_disabled("Move Column Left", column == 0, op(MoveColumnLeft))
            .menu_with_disabled(
                "Move Column Right",
                column + 1 >= model.column_count(),
                op(MoveColumnRight),
            )
            .separator()
            .menu("Edit as Markdown", op(EditAsMarkdown))
            .show(position, window, cx);
    }

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
        let last_row = table.rows.len().saturating_sub(1);
        let last_column = columns.saturating_sub(1);
        let grid = div()
            .text_size(px(TEXT_SIZE * CELL_SCALE))
            .line_height(relative(CELL_LINE_HEIGHT))
            .border_1()
            .border_color(theme.rule)
            .rounded(px(6.))
            .overflow_hidden()
            .children(rows);
        div()
            .id(("table", table_ix))
            .test_support()
            .group(TABLE_GROUP)
            .relative()
            .my(px(6.))
            .pr(px(HOVER_BUTTON))
            .pb(px(HOVER_BUTTON))
            .child(grid)
            .child(
                hover_button(
                    "add-column",
                    theme,
                    cx,
                    table_ix,
                    last_row,
                    last_column,
                    TableOpKind::InsertColumnRight,
                )
                .top_0()
                .bottom(px(HOVER_BUTTON))
                .right_0()
                .w(px(HOVER_BUTTON - 4.)),
            )
            .child(
                hover_button(
                    "add-row",
                    theme,
                    cx,
                    table_ix,
                    last_row,
                    last_column,
                    TableOpKind::InsertRowBelow,
                )
                .left_0()
                .right(px(HOVER_BUTTON))
                .bottom_0()
                .h(px(HOVER_BUTTON - 4.)),
            )
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
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    this.show_cell_menu(table_ix, r, c, event.position, window, cx);
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
