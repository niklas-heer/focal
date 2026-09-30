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
