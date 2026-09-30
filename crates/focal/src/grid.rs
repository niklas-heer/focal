//! Editing a table cell in place. The focused cell is a GPUI Kit `Input`; each
//! change rewrites the table's source through the buffer, and a whole cell
//! edit undoes as one step.

use focal_core::{EditKind, TableModel, editing::Change};
use gpui_kit::component::input::{InputEvent, InputState, Position};
use gpui_kit::{AppContext as _, Context, Entity, Subscription, Window};

use crate::editor::{CellExit, Editor};

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
