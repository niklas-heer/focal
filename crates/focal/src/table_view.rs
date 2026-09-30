//! Drawing a table as a grid: rendered cells, the cell being edited, and the
//! controls around the grid.

use focal_core::analysis::ColumnAlignment;
use focal_core::{TableModel, range_view};
use gpui_kit::component::input::Input;
use gpui_kit::component::native_menu::NativeMenu;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AppContext as _, Context, CursorStyle, FontWeight, InteractiveElement as _,
    IntoElement, MouseButton, MouseDownEvent, ParentElement as _, Render, ScrollHandle,
    SharedString, StatefulInteractiveElement as _, Styled as _, StyledText, TestSupportExt as _,
    TextRun, Window, canvas, div, font, point, px, relative,
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

/// Room above the grid for the column handles.
const HANDLE_ROOM: f32 = 16.;

/// A row being dragged by its handle.
#[derive(Clone, Copy)]
pub(crate) struct DraggedRow {
    table: usize,
    row: usize,
}

/// A column being dragged by its handle.
#[derive(Clone, Copy)]
pub(crate) struct DraggedColumn {
    table: usize,
    column: usize,
}

/// What follows the pointer while a row or column is dragged.
struct DragPreview(SharedString);

impl Render for DragPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px(px(8.))
            .py(px(2.))
            .rounded(px(4.))
            .bg(gpui_kit::rgba(0x7f7f_7f40))
            .text_size(px(12.))
            .child(self.0.clone())
    }
}

/// The grip at a body row's left edge, for dragging the row.
fn row_handle(table: usize, row: usize, theme: &Theme, cx: &Context<Editor>) -> impl IntoElement {
    div()
        .id(("row-handle", row))
        .test_support()
        .absolute()
        .top_0()
        .bottom_0()
        .left(px(-22.))
        .w(px(18.))
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(11.))
        .text_color(theme.marker)
        .opacity(0.)
        .group_hover(TABLE_GROUP, |style| style.opacity(1.))
        .cursor_grab()
        .child("⋮⋮")
        // Starting a drag must not open the table's first cell.
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_drag(DraggedRow { table, row }, |dragged, _, _, cx| {
            cx.new(|_| DragPreview(format!("Row {}", dragged.row).into()))
        })
        .on_drop(cx.listener(move |this, dragged: &DraggedRow, window, cx| {
            this.drop_row(dragged, table, row, window, cx);
        }))
}

/// The grip above a header cell, for dragging the column.
fn column_handle(table: usize, column: usize, theme: &Theme) -> impl IntoElement {
    div()
        .id(("column-handle", column))
        .test_support()
        .absolute()
        .left_0()
        .right_0()
        .top(px(-HANDLE_ROOM - 1.))
        .h(px(HANDLE_ROOM - 2.))
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(11.))
        .text_color(theme.marker)
        .opacity(0.)
        .group_hover(TABLE_GROUP, |style| style.opacity(1.))
        .cursor_grab()
        .child("⋯")
        // Starting a drag must not open the table's first cell.
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_drag(DraggedColumn { table, column }, |dragged, _, _, cx| {
            cx.new(|_| DragPreview(format!("Column {}", dragged.column + 1).into()))
        })
}

/// Cell text size relative to the body text.
const CELL_SCALE: f32 = 0.92;
/// gpui-kit's `Input` draws its line lower than `StyledText`, by about this
/// share of the font size; lifting it keeps the text in place when a cell is
/// edited.
const INPUT_LIFT: f32 = 0.24;
const CELL_LINE_HEIGHT: f32 = 1.45;
/// A cell's horizontal padding, plus room for the caret.
const CELL_PADDING: f32 = 22.;

/// Width a cell whose text is `text_width` wide asks of its column. Longer
/// cells wrap; a table wider than the text column scrolls sideways.
fn column_width(text_width: f32) -> f32 {
    (text_width + CELL_PADDING).clamp(64., 360.)
}

/// Scrolls a table sideways so the column is in view, clear of the handles'
/// room at the left edge.
fn reveal_column(scroll: &ScrollHandle, widths: &[f32], column: usize) {
    // The grid's 1 px border comes before the first column.
    let left = HOVER_BUTTON + 1. + widths[..column].iter().sum::<f32>();
    let right = left + widths.get(column).copied().unwrap_or(0.);
    let frame = f32::from(scroll.bounds().size.width);
    let shown = -f32::from(scroll.offset().x);
    let shown = if left - HOVER_BUTTON - 1. < shown {
        left - HOVER_BUTTON - 1.
    } else if right > shown + frame {
        right - frame
    } else {
        return;
    };
    scroll.set_offset(point(px(-shown.max(0.)), px(0.)));
}

/// The grid in its sideways scroll frame, with the add-row and add-column
/// buttons beside it.
fn table_frame(
    grid: gpui_kit::Div,
    scroll: &ScrollHandle,
    table_ix: usize,
    last_row: usize,
    last_column: usize,
    theme: &Theme,
    cx: &Context<Editor>,
) -> gpui_kit::Div {
    // Keeps the hover buttons beside a narrow table; a wide one
    // scrolls inside it.
    div()
        .relative()
        .flex_initial()
        .min_w(px(0.))
        .pr(px(HOVER_BUTTON))
        .pb(px(HOVER_BUTTON))
        .child(
            // The scroll frame clips, so it holds the drag handles'
            // room above and left of the grid.
            div()
                .id(("table-scroll", table_ix))
                .flex()
                .pt(px(HANDLE_ROOM))
                .pl(px(HOVER_BUTTON))
                .ml(px(-HOVER_BUTTON))
                .overflow_x_scroll()
                .track_scroll(scroll)
                .test_support()
                .child(grid),
        )
        // Once scrolled, the handles' room would show cells outside the
        // text column.
        .when(scroll.offset().x < px(0.), |d| {
            d.child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(px(-HOVER_BUTTON))
                    .w(px(HOVER_BUTTON))
                    .bg(theme.background),
            )
        })
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
            .top(px(HANDLE_ROOM))
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
}

impl Editor {
    fn drop_row(
        &mut self,
        dragged: &DraggedRow,
        table: usize,
        to: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if dragged.table == table && dragged.row != to {
            let op = TableOp {
                table,
                row: dragged.row,
                column: 0,
                op: TableOpKind::MoveRowTo(to),
            };
            self.table_op(&op, window, cx);
        }
    }

    fn drop_column(
        &mut self,
        dragged: &DraggedColumn,
        table: usize,
        to: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if dragged.table == table && dragged.column != to {
            let op = TableOp {
                table,
                row: 0,
                column: dragged.column,
                op: TableOpKind::MoveColumnTo(to),
            };
            self.table_op(&op, window, cx);
        }
    }

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

    /// Each column's width, from its widest cell as drawn.
    fn column_widths(
        &self,
        table_ix: usize,
        columns: usize,
        theme: &Theme,
        window: &Window,
    ) -> Vec<f32> {
        let font_size = px(TEXT_SIZE * CELL_SCALE);
        let analysis = &self.snapshot.analysis;
        let table = &analysis.tables[table_ix];
        let editing = self
            .grid
            .as_ref()
            .filter(|g| g.table == table_ix)
            .map(|g| (g.row, g.column));
        // The edited cell shows its source, markers included.
        (0..columns)
            .map(|c| {
                table
                    .rows
                    .iter()
                    .enumerate()
                    .filter_map(|(r, row)| {
                        let cell = row.get(c)?;
                        let (text, runs): (SharedString, _) = if editing == Some((r, c)) {
                            let text = &self.text()[cell.clone()];
                            let run = TextRun {
                                len: text.len(),
                                font: font(PROSE_FONT),
                                color: theme.text,
                                background_color: None,
                                underline: None,
                                strikethrough: None,
                            };
                            (text.to_owned().into(), vec![run])
                        } else {
                            let line = analysis.lines.line_of(cell.start);
                            let view = range_view(analysis, self.text(), line, cell.clone(), None);
                            let weight = if r == 0 {
                                FontWeight::BOLD
                            } else {
                                FontWeight::NORMAL
                            };
                            let runs = text_runs(
                                &view.runs, PROSE_FONT, weight, theme.text, theme, None, false,
                            );
                            (view.text.clone().into(), runs)
                        };
                        let width = window
                            .text_system()
                            .shape_line(text, font_size, &runs, None)
                            .width;
                        Some(column_width(f32::from(width)))
                    })
                    .fold(column_width(0.), f32::max)
            })
            .collect()
    }

    pub(crate) fn render_table(
        &self,
        table_ix: usize,
        theme: &Theme,
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let analysis = &self.snapshot.analysis;
        let table = &analysis.tables[table_ix];
        let columns = table.rows.iter().map(Vec::len).max().unwrap_or(0);
        let widths = self.column_widths(table_ix, columns, theme, window);
        let scroll = self
            .table_scrolls
            .borrow_mut()
            .entry(table_ix)
            .or_default()
            .clone();
        if let Some(grid) = self.grid.as_ref().filter(|g| g.table == table_ix)
            && self.reveal_cell.take()
        {
            reveal_column(&scroll, &widths, grid.column);
        }
        let painted = self.painted.clone();
        let drop_color = theme.selection;
        let rows = (0..table.rows.len()).map(|r| {
            div()
                .id(("row", r))
                .relative()
                .flex()
                .when(r > 0, |d| d.border_t_1())
                .when(r == 0, |d| d.bg(theme.code_background).rounded_t(px(5.)))
                .border_color(theme.rule)
                .when(r > 0, |d| {
                    d.drag_over::<DraggedRow>(move |style, dragged: &DraggedRow, _, _| {
                        if dragged.table == table_ix {
                            style.bg(drop_color)
                        } else {
                            style
                        }
                    })
                    .on_drop(cx.listener(move |this, dragged: &DraggedRow, window, cx| {
                        this.drop_row(dragged, table_ix, r, window, cx);
                    }))
                    .child(row_handle(table_ix, r, theme, cx))
                })
                .children(
                    (0..columns).map(|c| self.render_cell(table_ix, r, c, widths[c], theme, cx)),
                )
        });
        let last_row = table.rows.len().saturating_sub(1);
        let last_column = columns.saturating_sub(1);
        let grid = div()
            .flex_none()
            .text_size(px(TEXT_SIZE * CELL_SCALE))
            .line_height(relative(CELL_LINE_HEIGHT))
            .border_1()
            .border_color(theme.rule)
            .rounded(px(6.))
            .children(rows);
        let element = div()
            .id(("table", table_ix))
            .test_support()
            .group(TABLE_GROUP)
            .relative()
            .py(px(6.))
            .flex()
            .child(table_frame(
                grid,
                &scroll,
                table_ix,
                last_row,
                last_column,
                theme,
                cx,
            ))
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
            .into_any_element();
        // A table in a quote or list keeps its quote bars and indentation.
        let line = table.lines.start;
        let entity = cx.entity().downgrade();
        crate::prefix::wrap(
            element,
            &analysis.info(line).prefix,
            line,
            theme,
            px(TEXT_SIZE * 1.6),
            move |line, _, cx| {
                entity
                    .update(cx, |editor, cx| editor.toggle_task_on_line(line, cx))
                    .ok();
            },
        )
    }

    fn render_cell(
        &self,
        table_ix: usize,
        r: usize,
        c: usize,
        width: f32,
        theme: &Theme,
        cx: &Context<Self>,
    ) -> AnyElement {
        let analysis = &self.snapshot.analysis;
        let table = &analysis.tables[table_ix];
        let drop_color = theme.selection;
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
            .flex_none()
            .w(px(width))
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
            .relative()
            .drag_over::<DraggedColumn>(move |style, dragged: &DraggedColumn, _, _| {
                if dragged.table == table_ix {
                    style.bg(drop_color)
                } else {
                    style
                }
            })
            .on_drop(
                cx.listener(move |this, dragged: &DraggedColumn, window, cx| {
                    this.drop_column(dragged, table_ix, c, window, cx);
                }),
            )
            .when(r == 0, |d| d.child(column_handle(table_ix, c, theme)))
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
                    div()
                        .key_context("FocalCell")
                        .h(px(TEXT_SIZE * CELL_SCALE * CELL_LINE_HEIGHT))
                        .overflow_hidden()
                        .child(
                            // Match the rendered cell so the row keeps its size
                            // while its cell is edited.
                            Input::new(&input)
                                .appearance(false)
                                .p(px(0.))
                                .h(px(TEXT_SIZE * CELL_SCALE * CELL_LINE_HEIGHT))
                                .text_size(px(TEXT_SIZE * CELL_SCALE))
                                .line_height(relative(CELL_LINE_HEIGHT))
                                .font_family(PROSE_FONT)
                                .mt(px(-TEXT_SIZE * CELL_SCALE * INPUT_LIFT))
                                .when(alignment == Some(ColumnAlignment::Center), |d| {
                                    d.text_center()
                                })
                                .when(alignment == Some(ColumnAlignment::Right), |d| {
                                    d.text_right()
                                }),
                        ),
                ),
                None => d.children(content),
            })
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::column_width;

    #[test]
    fn columns_fit_their_widest_cell_within_bounds() {
        assert!(
            (column_width(0.) - 64.).abs() < f32::EPSILON,
            "narrow columns keep a minimum"
        );
        assert!((column_width(100.) - 122.).abs() < f32::EPSILON);
        assert!(
            (column_width(1000.) - 360.).abs() < f32::EPSILON,
            "long cells wrap instead"
        );
    }
}
