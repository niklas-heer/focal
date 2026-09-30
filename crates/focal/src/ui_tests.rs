//! Headless UI tests: the real editor in a test window, driven by key presses,
//! text input and clicks through GPUI Kit's test harness.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, Bounds, Entity, Focusable as _, TestAppContext, Window,
    WindowBounds, WindowOptions, point, px, size,
};

use crate::document::Document;
use crate::editor::{self, Editor};

pub fn open_editor(cx: &mut TestAppContext, text: &str) -> (AnyWindowHandle, Entity<Editor>) {
    let text = text.to_owned();
    cx.update(|cx| {
        gpui_kit::init(cx);
        editor::bind_keys(cx);
        let bounds = Bounds {
            origin: point(px(0.), px(0.)),
            size: size(px(900.), px(700.)),
        };
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            ..WindowOptions::default()
        };
        gpui_kit::open_window(options, cx, |window, cx| {
            let editor = cx.new(|cx| Editor::new(Document::untitled(), text, window, cx));
            window.focus(&editor.read(cx).focus_handle(cx), cx);
            editor
        })
        .expect("open test window")
    })
}

/// Renders a frame, then runs `f` with the window.
pub fn act(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    f: impl FnOnce(&mut Window, &mut App),
) {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        f(window, cx);
    })
    .expect("window is open");
}

#[gpui_kit::test]
fn typing_inserts_at_the_caret(cx: &mut TestAppContext) {
    let (window, editor) = open_editor(cx, "Hello");
    act(cx, window, |window, cx| {
        window.press("cmd-down", cx);
        window.input(" world", cx);
    });
    editor.read_with(cx, |editor, _| {
        assert_eq!(editor.text(), "Hello world");
        assert_eq!(
            editor.selection(),
            11..11,
            "the caret follows the typed text"
        );
    });
}

#[gpui_kit::test]
fn return_continues_a_list(cx: &mut TestAppContext) {
    let (window, editor) = open_editor(cx, "- one");
    act(cx, window, |window, cx| {
        window.press("cmd-down", cx);
        window.press("enter", cx);
        window.input("two", cx);
    });
    editor.read_with(cx, |editor, _| assert_eq!(editor.text(), "- one\n- two"));
}

#[gpui_kit::test]
fn clicking_a_checkbox_toggles_the_task(cx: &mut TestAppContext) {
    let (window, editor) = open_editor(cx, "- [ ] buy milk\n- [x] done");
    act(cx, window, |window, cx| window.click(("task", 0usize), cx));
    editor.read_with(cx, |editor, _| {
        assert_eq!(editor.text(), "- [x] buy milk\n- [x] done");
    });
    act(cx, window, |window, cx| window.click(("task", 1usize), cx));
    editor.read_with(cx, |editor, _| {
        assert_eq!(editor.text(), "- [x] buy milk\n- [ ] done");
    });
}

#[gpui_kit::test]
fn arrow_left_from_an_item_skips_its_marker(cx: &mut TestAppContext) {
    let (window, editor) = open_editor(cx, "intro\n- item");
    act(cx, window, |window, cx| {
        window.press("cmd-down", cx);
        window.press("cmd-left", cx);
    });
    editor.read_with(cx, |editor, _| assert_eq!(editor.selection(), 8..8));
    act(cx, window, |window, cx| window.press("left", cx));
    editor.read_with(cx, |editor, _| assert_eq!(editor.selection(), 5..5));
}

#[gpui_kit::test]
fn backspace_at_an_item_start_removes_the_bullet(cx: &mut TestAppContext) {
    let (window, editor) = open_editor(cx, "- item");
    act(cx, window, |window, cx| {
        window.press("cmd-down", cx);
        window.press("cmd-left", cx);
        window.press("backspace", cx);
    });
    editor.read_with(cx, |editor, _| assert_eq!(editor.text(), "item"));
}

#[gpui_kit::test]
fn tab_and_shift_tab_change_the_list_level(cx: &mut TestAppContext) {
    let (window, editor) = open_editor(cx, "- a\n- b");
    act(cx, window, |window, cx| {
        window.press("cmd-down", cx);
        window.press("tab", cx);
    });
    editor.read_with(cx, |editor, _| assert_eq!(editor.text(), "- a\n  - b"));
    act(cx, window, |window, cx| window.press("shift-tab", cx));
    editor.read_with(cx, |editor, _| assert_eq!(editor.text(), "- a\n- b"));
}

#[gpui_kit::test]
fn down_into_a_table_edits_its_first_cell(cx: &mut TestAppContext) {
    let text = "before\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\nafter";
    let (window, editor) = open_editor(cx, text);
    act(cx, window, |window, cx| {
        window.press("down", cx);
        window.press("down", cx);
    });
    act(cx, window, |window, cx| window.input("x", cx));
    editor.read_with(cx, |editor, _| {
        assert!(editor.text().contains("| ax  | b   |"), "{}", editor.text());
    });
}

#[gpui_kit::test]
fn undo_restores_text_after_list_editing(cx: &mut TestAppContext) {
    let (window, editor) = open_editor(cx, "- a");
    act(cx, window, |window, cx| {
        window.press("cmd-down", cx);
        window.press("enter", cx);
        window.input("b", cx);
        window.press("cmd-z", cx);
        window.press("cmd-z", cx);
    });
    editor.read_with(cx, |editor, _| assert_eq!(editor.text(), "- a"));
}

fn click_cell(window: &mut Window, cx: &mut App, table: usize, row: usize, column: usize) {
    window
        .within(("table", table))
        .within(("row", row))
        .click(("cell", column), cx);
}

#[gpui_kit::test]
fn typing_in_a_cell_rewrites_the_table_aligned(cx: &mut TestAppContext) {
    let (window, editor) = open_editor(cx, "| a | b |\n|---|---|\n| 1 | 2 |\n");
    act(cx, window, |window, cx| {
        click_cell(window, cx, 0, 1, 1);
        window.press("cmd-a", cx);
        window.input("wide value", cx);
    });
    editor.read_with(cx, |editor, _| {
        assert_eq!(
            editor.text(),
            "| a   | b          |\n| --- | ---------- |\n| 1   | wide value |\n"
        );
    });
}

#[gpui_kit::test]
fn one_undo_restores_the_table_after_a_cell_edit(cx: &mut TestAppContext) {
    let text = "| a | b |\n|---|---|\n| 1 | 2 |\n";
    let (window, editor) = open_editor(cx, text);
    act(cx, window, |window, cx| {
        click_cell(window, cx, 0, 1, 0);
        window.input("0", cx);
        window.press("escape", cx);
        window.press("cmd-z", cx);
    });
    editor.read_with(cx, |editor, _| assert_eq!(editor.text(), text));
}

#[gpui_kit::test]
fn tables_never_show_their_source(cx: &mut TestAppContext) {
    let (window, editor) = open_editor(cx, "above\n\n| a |\n|---|\n| 1 |\n");
    act(cx, window, |window, cx| {
        window.press("down", cx);
        window.press("down", cx);
        assert!(
            window.try_find(("table", 0usize)).is_some(),
            "the grid is still drawn"
        );
    });
    let _ = editor;
}

#[gpui_kit::test]
fn tab_and_return_move_through_cells_and_add_rows(cx: &mut TestAppContext) {
    let (window, editor) = open_editor(cx, "| a | b |\n|---|---|\n| 1 | 2 |\n");
    // Each step gets its own frame: a newly focused cell's input receives
    // text only once it has been drawn, as it always is between key presses.
    act(cx, window, |window, cx| {
        click_cell(window, cx, 0, 1, 0);
        window.press("tab", cx);
    });
    act(cx, window, |window, cx| window.press("cmd-a", cx));
    act(cx, window, |window, cx| {
        window.input("B", cx);
        window.press("enter", cx);
    });
    act(cx, window, |window, cx| window.input("new", cx));
    editor.read_with(cx, |editor, _| {
        assert_eq!(
            editor.text(),
            "| a   | b   |\n| --- | --- |\n| 1   | B   |\n|     | new |\n"
        );
    });
}

#[gpui_kit::test]
fn arrows_enter_and_leave_the_grid(cx: &mut TestAppContext) {
    let (window, editor) = open_editor(cx, "above\n\n| a |\n|---|\n| 1 |\n\nbelow");
    act(cx, window, |window, cx| {
        window.press("down", cx);
        window.press("down", cx);
    });
    act(cx, window, |window, cx| window.input("h", cx));
    editor.read_with(cx, |editor, _| {
        assert!(editor.text().contains("| ah  |"), "{}", editor.text());
    });
    act(cx, window, |window, cx| window.press("down", cx));
    act(cx, window, |window, cx| window.press("down", cx));
    act(cx, window, |window, cx| window.input("!", cx));
    // ↓ from the last row leaves to the start of the line after the table.
    editor.read_with(cx, |editor, _| {
        assert_eq!(
            editor.text(),
            "above\n\n| ah  |\n| --- |\n| 1   |\n!\nbelow"
        );
    });
}

fn table_op(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    row: usize,
    column: usize,
    op: crate::grid::TableOpKind,
) {
    act(cx, window, |window, cx| {
        window.dispatch_action(
            Box::new(crate::grid::TableOp {
                table: 0,
                row,
                column,
                op,
            }),
            cx,
        );
    });
}

#[gpui_kit::test]
fn row_and_column_operations_rewrite_the_table(cx: &mut TestAppContext) {
    use crate::grid::TableOpKind::*;
    let (window, editor) = open_editor(cx, "| a | b |\n|---|---|\n| 1 | 2 |\n");
    act(cx, window, |window, cx| click_cell(window, cx, 0, 1, 0));
    table_op(cx, window, 1, 0, InsertRowBelow);
    table_op(cx, window, 1, 1, InsertColumnRight);
    table_op(cx, window, 1, 0, AlignRight);
    editor.read_with(cx, |editor, _| {
        assert_eq!(
            editor.text(),
            "|   a | b   |     |\n| --: | --- | --- |\n|   1 | 2   |     |\n|     |     |     |\n"
        );
    });
    table_op(cx, window, 2, 2, DeleteColumn);
    table_op(cx, window, 2, 0, DeleteRow);
    editor.read_with(cx, |editor, _| {
        assert_eq!(
            editor.text(),
            "|   a | b   |\n| --: | --- |\n|   1 | 2   |\n"
        );
    });
}

#[gpui_kit::test]
fn edit_as_markdown_shows_the_source_until_the_caret_leaves(cx: &mut TestAppContext) {
    use crate::grid::TableOpKind::*;
    let (window, editor) = open_editor(cx, "| a |\n|---|\n| 1 |\n\nafter");
    table_op(cx, window, 0, 0, EditAsMarkdown);
    act(cx, window, |window, cx| {
        assert!(
            window.try_find(("table", 0usize)).is_none(),
            "shown as source"
        );
        window.input("x", cx);
    });
    editor.read_with(cx, |editor, _| assert!(editor.text().starts_with("x| a |")));
}

#[gpui_kit::test]
fn dragging_a_row_handle_moves_the_row(cx: &mut TestAppContext) {
    let (window, editor) = open_editor(cx, "| h |\n|---|\n| 1 |\n| 2 |\n| 3 |\n");
    // The handles show while the pointer is over the table.
    act(cx, window, |window, cx| window.hover(("table", 0usize), cx));
    act(cx, window, |window, cx| {
        window.within(("table", 0usize)).drag_to(
            ("row-handle", 3usize),
            ("row-handle", 1usize),
            cx,
        );
    });
    editor.read_with(cx, |editor, _| {
        assert_eq!(
            editor.text(),
            "| h   |\n| --- |\n| 3   |\n| 1   |\n| 2   |\n"
        );
    });
}

#[gpui_kit::test]
fn jumping_to_the_end_of_a_long_document_shows_the_caret(cx: &mut TestAppContext) {
    let text = "Line\n\n".repeat(400);
    let (window, editor) = open_editor(cx, &text);
    act(cx, window, |window, cx| window.press("cmd-down", cx));
    // Rows below the viewport are measured only as they are laid out.
    for _ in 0..3 {
        act(cx, window, |_, _| {});
    }
    editor.read_with(cx, |editor, _| {
        let bounds = editor
            .head_row_bounds()
            .expect("the caret's row is laid out");
        assert!(
            bounds.bottom() <= px(700.),
            "the caret's row ends at {:?}, below the window",
            bounds.bottom()
        );
    });
}

#[gpui_kit::test]
fn dragging_a_column_handle_moves_the_column(cx: &mut TestAppContext) {
    let (window, editor) = open_editor(cx, "> | Key | Action |\n> |:--|:--|\n> | z | undo |\n");
    act(cx, window, |window, cx| window.hover(("table", 0usize), cx));
    act(cx, window, |window, cx| {
        window
            .within(("table", 0usize))
            .within(("row", 0usize))
            .drag_to(("column-handle", 1usize), ("cell", 0usize), cx);
    });
    act(cx, window, |_, _| {});
    editor.read_with(cx, |editor, _| {
        assert_eq!(
            editor.text(),
            "> | Action | Key |\n> | :----- | :-- |\n> | undo   | z   |\n"
        );
    });
}

#[gpui_kit::test]
fn pressing_a_drag_handle_does_not_edit_a_cell(cx: &mut TestAppContext) {
    let (window, editor) = open_editor(cx, "| a | b |\n|---|---|\n| 1 | 2 |\n");
    act(cx, window, |window, cx| window.hover(("table", 0usize), cx));
    act(cx, window, |window, cx| {
        window
            .within(("table", 0usize))
            .click(("column-handle", 1usize), cx);
    });
    act(cx, window, |window, cx| {
        window
            .within(("table", 0usize))
            .click(("row-handle", 1usize), cx);
    });
    editor.read_with(cx, |editor, _| assert_eq!(editor.editing_cell(), None));
}

#[gpui_kit::test]
fn undo_in_a_focused_cell_restores_the_table(cx: &mut TestAppContext) {
    let text = "| a | b |\n|---|---|\n| 1 | 2 |\n";
    let (window, editor) = open_editor(cx, text);
    act(cx, window, |window, cx| click_cell(window, cx, 0, 1, 0));
    act(cx, window, |window, cx| window.input("0", cx));
    act(cx, window, |_, _| {});
    editor.read_with(cx, |editor, _| {
        assert_eq!(
            editor.text(),
            "| a   | b   |\n| --- | --- |\n| 10  | 2   |\n"
        );
    });
    act(cx, window, |window, cx| window.press("cmd-z", cx));
    act(cx, window, |_, _| {});
    editor.read_with(cx, |editor, _| assert_eq!(editor.text(), text));
}

fn cell_bounds(
    window: &mut Window,
    table: usize,
    row: usize,
    column: usize,
) -> Bounds<gpui_kit::Pixels> {
    window
        .within(("table", table))
        .within(("row", row))
        .find(("cell", column))
        .bounds()
}

#[gpui_kit::test]
fn tab_into_a_hidden_cell_scrolls_it_into_view(cx: &mut TestAppContext) {
    let cell = "a long cell that takes up space";
    let row = |text: &str| format!("|{}\n", format!(" {text} |").repeat(8));
    let wide = format!("{}{}{}", row("h"), "|---".repeat(8) + "|\n", row(cell));
    // A table drawn before the edited one must not take its reveal.
    let text = format!("| a |\n|---|\n| 1 |\n\n{wide}");
    let (window, _) = open_editor(cx, &text);
    act(cx, window, |window, cx| click_cell(window, cx, 1, 1, 0));
    for _ in 0..7 {
        act(cx, window, |window, cx| window.press("tab", cx));
    }
    act(cx, window, |_, _| {});
    act(cx, window, |window, _| {
        let frame = window.find(("table-scroll", 1usize)).bounds();
        let cell = cell_bounds(window, 1, 1, 7);
        assert!(
            cell.left() >= frame.left() && cell.right() <= frame.right(),
            "the edited cell {cell:?} is inside the table's frame {frame:?}"
        );
    });
}
