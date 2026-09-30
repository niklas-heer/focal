//! Editing a table cell in place. The focused cell is a GPUI Kit `Input`; each
//! change rewrites the table's source through the buffer, and a whole cell
//! edit undoes as one step.

use focal_core::analysis::ColumnAlignment;
use focal_core::{EditKind, TableModel, editing::Change};
use gpui_kit::component::input::{InputEvent, InputState, Position};
use gpui_kit::{AppContext as _, Context, Entity, Subscription, Window};

use crate::editor::{CellBelow, CellDown, CellExit, CellNext, CellPrevious, CellUp, Editor};

/// A row or column operation on a table, from its menu or hover controls.
#[derive(Clone, Debug, PartialEq, gpui_kit::Action)]
#[action(namespace = focal, no_json)]
pub struct TableOp {
    pub table: usize,
    pub row: usize,
    pub column: usize,
    pub op: TableOpKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableOpKind {
    InsertRowAbove,
    InsertRowBelow,
    DeleteRow,
    InsertColumnLeft,
    InsertColumnRight,
    DeleteColumn,
    AlignLeft,
    AlignCenter,
    AlignRight,
    AlignNone,
    MoveRowUp,
    MoveRowDown,
    MoveColumnLeft,
    MoveColumnRight,
    /// Move the row to this index, from a drag.
    MoveRowTo(usize),
    /// Move the column to this index, from a drag.
    MoveColumnTo(usize),
    EditAsMarkdown,
}

pub(crate) struct GridSession {
    pub table: usize,
    pub row: usize,
    pub column: usize,
    pub input: Entity<InputState>,
    pub session: u64,
    _subscription: Subscription,
}

impl Editor {
    /// Starts editing a cell, ending any other cell edit.
    pub(crate) fn edit_cell(
        &mut self,
        table: usize,
        row: usize,
        column: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The input reports changes as deferred events; write the cell being
        // left before its session is replaced, so no keystroke is lost.
        self.cell_changed(window, cx);
        let Some(model) = TableModel::from_analysis(&self.snapshot.analysis, self.text(), table)
        else {
            return;
        };
        if row >= model.rows.len() || column >= model.column_count() {
            return;
        }
        let value = model.cell(row, column).to_owned();
        let input = cx.new(|cx| InputState::new(window, cx).default_value(value.clone()));
        // Clicking a cell puts the cursor at the end of its text.
        let end = u32::try_from(value.chars().count()).unwrap_or(u32::MAX);
        input.update(cx, |input, cx| {
            input.set_cursor_position(Position::new(0, end), window, cx);
        });
        let subscription =
            cx.subscribe_in(&input, window, |this, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::Change) {
                    this.cell_changed(window, cx);
                }
            });
        input.update(cx, |input, cx| input.focus(window, cx));
        self.next_grid_session += 1;
        self.grid = Some(GridSession {
            table,
            row,
            column,
            input,
            session: self.next_grid_session,
            _subscription: subscription,
        });
        cx.notify();
    }

    fn cell_changed(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let Some(grid) = &self.grid else { return };
        let (table, row, column, session) = (grid.table, grid.row, grid.column, grid.session);
        let value = grid.input.read(cx).value().to_string();
        let analysis = self.snapshot.analysis.clone();
        let Some(mut model) = TableModel::from_analysis(&analysis, self.text(), table) else {
            return;
        };
        if model.cell(row, column) == value {
            return;
        }
        model.set_cell(row, column, &value);
        if let Some(change) = model.replace(&analysis, self.text(), table) {
            self.apply_in_grid(&change, EditKind::Grid(session), cx);
        }
    }

    /// Applies a table change while keeping the text caret on the same text.
    pub(crate) fn apply_in_grid(
        &mut self,
        change: &Change,
        kind: EditKind,
        cx: &mut Context<Self>,
    ) {
        let selection = self.shifted_selection(change);
        self.edit_keeping_grid(change.range.clone(), &change.text, selection, kind, cx);
    }

    /// Ends cell editing and puts the caret at `to` in the text.
    pub(crate) fn leave_grid(&mut self, to: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.cell_changed(window, cx);
        self.grid = None;
        window.focus(&self.focus_handle, cx);
        self.move_to(to, cx);
    }

    pub(crate) fn cell_exit(&mut self, _: &CellExit, window: &mut Window, cx: &mut Context<Self>) {
        let Some(grid) = &self.grid else { return };
        let to = self.snapshot.analysis.tables[grid.table].range.end;
        self.leave_grid(to, window, cx);
    }
}

impl Editor {
    fn grid_model(&self) -> Option<(usize, usize, usize, TableModel)> {
        let grid = self.grid.as_ref()?;
        let model = TableModel::from_analysis(&self.snapshot.analysis, self.text(), grid.table)?;
        Some((grid.table, grid.row, grid.column, model))
    }

    /// Adds an empty row at `at` and returns whether it worked.
    fn add_row(&mut self, table: usize, at: usize, cx: &mut Context<Self>) -> bool {
        let analysis = self.snapshot.analysis.clone();
        let Some(mut model) = TableModel::from_analysis(&analysis, self.text(), table) else {
            return false;
        };
        model.insert_row(at);
        let Some(change) = model.replace(&analysis, self.text(), table) else {
            return false;
        };
        self.apply_in_grid(&change, EditKind::Other, cx);
        true
    }

    pub(crate) fn cell_next(&mut self, _: &CellNext, window: &mut Window, cx: &mut Context<Self>) {
        let Some((table, row, column, model)) = self.grid_model() else {
            return;
        };
        if column + 1 < model.column_count() {
            self.edit_cell(table, row, column + 1, window, cx);
        } else if row + 1 < model.rows.len() || self.add_row(table, row + 1, cx) {
            self.edit_cell(table, row + 1, 0, window, cx);
        }
    }

    pub(crate) fn cell_previous(
        &mut self,
        _: &CellPrevious,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((table, row, column, model)) = self.grid_model() else {
            return;
        };
        if column > 0 {
            self.edit_cell(table, row, column - 1, window, cx);
        } else if row > 0 {
            self.edit_cell(table, row - 1, model.column_count() - 1, window, cx);
        }
    }

    pub(crate) fn cell_below(
        &mut self,
        _: &CellBelow,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((table, row, column, model)) = self.grid_model() else {
            return;
        };
        if row + 1 < model.rows.len() || self.add_row(table, row + 1, cx) {
            self.edit_cell(table, row + 1, column, window, cx);
        }
    }

    pub(crate) fn cell_up(&mut self, _: &CellUp, window: &mut Window, cx: &mut Context<Self>) {
        let Some((table, row, column, _)) = self.grid_model() else {
            return;
        };
        if row > 0 {
            self.edit_cell(table, row - 1, column, window, cx);
        } else {
            let analysis = &self.snapshot.analysis;
            let first = analysis.tables[table].lines.start;
            let to = if first > 0 {
                analysis.content_range(first - 1).end
            } else {
                0
            };
            self.leave_grid(to, window, cx);
        }
    }

    pub(crate) fn cell_down(&mut self, _: &CellDown, window: &mut Window, cx: &mut Context<Self>) {
        let Some((table, row, column, model)) = self.grid_model() else {
            return;
        };
        if row + 1 < model.rows.len() {
            self.edit_cell(table, row + 1, column, window, cx);
        } else {
            let analysis = &self.snapshot.analysis;
            let after = analysis.tables[table].lines.end;
            let to = if after < analysis.line_count() {
                analysis.content_range(after).start
            } else {
                self.text().len()
            };
            self.leave_grid(to, window, cx);
        }
    }

    pub(crate) fn table_op(
        &mut self,
        action: &TableOp,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use TableOpKind::*;
        // Keep what was typed, then close the cell: its row and column may
        // hold another cell once the table changes shape.
        self.cell_changed(window, cx);
        self.grid = None;
        let (table, row, column) = (action.table, action.row, action.column);
        if action.op == EditAsMarkdown {
            self.grid = None;
            self.table_source = Some(table);
            let start = self.snapshot.analysis.tables[table].range.start;
            window.focus(&self.focus_handle, cx);
            self.move_to(start, cx);
            return;
        }
        let analysis = self.snapshot.analysis.clone();
        let Some(mut model) = TableModel::from_analysis(&analysis, self.text(), table) else {
            return;
        };
        let (mut row, mut column) = (row, column);
        match action.op {
            InsertRowAbove => model.insert_row(row.max(1)),
            InsertRowBelow => {
                model.insert_row(row + 1);
                row += 1;
            }
            DeleteRow => {
                model.delete_row(row);
                row = row.saturating_sub(1).max(usize::from(model.rows.len() > 1));
            }
            InsertColumnLeft => model.insert_column(column),
            InsertColumnRight => {
                model.insert_column(column + 1);
                column += 1;
            }
            DeleteColumn => {
                model.delete_column(column);
                column = column.min(model.column_count() - 1);
            }
            AlignLeft => model.set_alignment(column, ColumnAlignment::Left),
            AlignCenter => model.set_alignment(column, ColumnAlignment::Center),
            AlignRight => model.set_alignment(column, ColumnAlignment::Right),
            AlignNone => model.set_alignment(column, ColumnAlignment::None),
            MoveRowUp => {
                model.move_row(row, row.saturating_sub(1));
                row = row.saturating_sub(1).max(1);
            }
            MoveRowDown => {
                model.move_row(row, row + 1);
                row = (row + 1).min(model.rows.len() - 1);
            }
            MoveColumnLeft => {
                model.move_column(column, column.saturating_sub(1));
                column = column.saturating_sub(1);
            }
            MoveColumnRight => {
                model.move_column(column, column + 1);
                column = (column + 1).min(model.column_count() - 1);
            }
            MoveRowTo(to) => {
                model.move_row(row, to);
                row = to.clamp(1, model.rows.len().saturating_sub(1).max(1));
            }
            MoveColumnTo(to) => {
                model.move_column(column, to);
                column = to.min(model.column_count() - 1);
            }
            EditAsMarkdown => {}
        }
        if let Some(change) = model.replace(&analysis, self.text(), table) {
            self.apply_in_grid(&change, EditKind::Other, cx);
        }
        self.edit_cell(table, row, column, window, cx);
    }
}
