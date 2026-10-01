//! Headless UI tests: the real editor in a test window, driven by key presses,
//! text input and clicks through GPUI Kit's test harness.

use gpui_kit::BorrowAppContext as _;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, Bounds, Entity, TestAppContext, Window, WindowBounds,
    WindowOptions, point, px, size,
};

use crate::document::Document;
use crate::editor::{self, Editor};
use crate::settings::Settings;
use crate::workspace::Workspace;

pub fn open_workspace(cx: &mut TestAppContext, text: &str) -> (AnyWindowHandle, Entity<Workspace>) {
    open_document(cx, Document::untitled(), text)
}

/// A window showing `document`, whose text is `text`.
pub fn open_document(
    cx: &mut TestAppContext,
    document: Document,
    text: &str,
) -> (AnyWindowHandle, Entity<Workspace>) {
    let text = text.to_owned();
    cx.update(|cx| {
        gpui_kit::init(cx);
        editor::bind_keys(cx);
        crate::workspace::bind_keys(cx);
        crate::find_bar::bind_keys(cx);
        // Tests never read or write the user's settings file.
        cx.set_global(Settings::default());
        let bounds = Bounds {
            origin: point(px(0.), px(0.)),
            size: size(px(900.), px(700.)),
        };
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            ..WindowOptions::default()
        };
        gpui_kit::open_window(options, cx, |window, cx| {
            let workspace = cx.new(|cx| Workspace::new(document, text, window, cx));
            Workspace::focus_editor(&workspace, window, cx);
            workspace
        })
        .expect("open test window")
    })
}

pub fn open_editor(cx: &mut TestAppContext, text: &str) -> (AnyWindowHandle, Entity<Editor>) {
    let (window, workspace) = open_workspace(cx, text);
    let editor = workspace.read_with(cx, |workspace, _| workspace.editor().clone());
    (window, editor)
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

#[gpui_kit::test]
fn the_workspace_focuses_its_editor(cx: &mut TestAppContext) {
    let (window, workspace) = open_workspace(cx, "text");
    act(cx, window, |window, cx| {
        let editor = workspace.read(cx).editor().clone();
        assert!(editor.read(cx).focus_handle.is_focused(window));
    });
}

fn bar_shown(window: &mut Window) -> bool {
    window
        .try_find("bottom-bar")
        .is_some_and(|bar| bar.visible())
}

#[gpui_kit::test]
fn the_bar_shows_near_the_bottom_and_hides_while_typing(cx: &mut TestAppContext) {
    let (window, _) = open_workspace(cx, "text");
    act(cx, window, |window, _| {
        assert!(!bar_shown(window), "hidden at first");
    });
    act(cx, window, |window, cx| window.hover("bar-zone", cx));
    act(cx, window, |window, _| {
        assert!(bar_shown(window), "the pointer is near the bottom");
    });
    act(cx, window, |window, cx| window.input("x", cx));
    act(cx, window, |window, _| {
        assert!(!bar_shown(window), "typing hides it");
    });
}

#[gpui_kit::test]
fn bar_buttons_format_the_selection(cx: &mut TestAppContext) {
    let (window, workspace) = open_workspace(cx, "word");
    act(cx, window, |window, cx| window.press("cmd-a", cx));
    act(cx, window, |window, cx| window.hover("bar-zone", cx));
    act(cx, window, |window, cx| window.click("bar-bold", cx));
    act(cx, window, |window, cx| window.click("bar-quote", cx));
    let editor = workspace.read_with(cx, |w, _| w.editor().clone());
    editor.read_with(cx, |editor, _| assert_eq!(editor.text(), "> **word**"));
}

#[gpui_kit::test]
fn heading_keys_and_the_word_count(cx: &mut TestAppContext) {
    let (window, editor) = open_editor(cx, "two words");
    act(cx, window, |window, cx| window.press("cmd-2", cx));
    editor.read_with(cx, |editor, _| {
        assert_eq!(editor.text(), "## two words");
        let state = editor.bar_state();
        assert_eq!((state.heading, state.words, state.minutes), (2, 2, 1));
    });
    act(cx, window, |window, cx| window.press("cmd-0", cx));
    editor.read_with(cx, |editor, _| assert_eq!(editor.text(), "two words"));
}

fn set_settings(cx: &mut TestAppContext, change: impl FnOnce(&mut Settings)) {
    cx.update(|cx| {
        cx.update_global::<Settings, _>(|settings, _| change(settings));
    });
}

#[gpui_kit::test]
fn focus_mode_keeps_the_caret_sentence_or_paragraph(cx: &mut TestAppContext) {
    let text = "One. Two. Three.\n\nNext paragraph.";
    let (window, editor) = open_editor(cx, text);
    set_settings(cx, |s| s.focus_unit = crate::settings::FocusUnit::Sentence);
    editor.update(cx, |editor, cx| editor.move_to(6, cx));
    act(cx, window, |window, cx| window.press("cmd-d", cx));
    editor.read_with(cx, |editor, _| assert_eq!(editor.focus_range(), Some(5..9)));
    set_settings(cx, |s| s.focus_unit = crate::settings::FocusUnit::Paragraph);
    act(cx, window, |_, _| {});
    editor.read_with(cx, |editor, _| {
        assert_eq!(editor.focus_range(), Some(0..16));
    });
}

#[gpui_kit::test]
fn focus_mode_hides_the_bar(cx: &mut TestAppContext) {
    let (window, _) = open_workspace(cx, "text");
    act(cx, window, |window, cx| window.hover("bar-zone", cx));
    act(cx, window, |window, _| assert!(bar_shown(window)));
    act(cx, window, |window, cx| {
        window.dispatch_action(Box::new(editor::ToggleFocusMode), cx);
    });
    act(cx, window, |window, _| assert!(!bar_shown(window)));
}

#[gpui_kit::test]
fn typewriter_scrolling_centers_the_caret_line(cx: &mut TestAppContext) {
    let (window, editor) = open_editor(cx, &"Line\n".repeat(80));
    act(cx, window, |window, cx| window.press("cmd-d", cx));
    act(cx, window, |window, cx| {
        for _ in 0..30 {
            window.press("down", cx);
        }
    });
    for _ in 0..4 {
        act(cx, window, |_, _| {});
    }
    editor.read_with(cx, |editor, _| {
        let row = editor
            .head_row_bounds()
            .expect("the caret's row is laid out");
        let viewport = editor.viewport();
        let offset = (row.center().y - viewport.center().y).abs();
        assert!(
            offset <= px(2.),
            "the caret line is {offset:?} from the center"
        );
    });
}

#[gpui_kit::test]
fn a_cell_edit_in_focus_mode_does_not_scroll(cx: &mut TestAppContext) {
    let text = format!("intro\n\n| a |\n|---|\n| 1 |\n{}", "x\n".repeat(60));
    let (window, editor) = open_editor(cx, &text);
    act(cx, window, |window, cx| window.press("cmd-d", cx));
    for _ in 0..3 {
        act(cx, window, |_, _| {});
    }
    act(cx, window, |window, cx| click_cell(window, cx, 0, 1, 0));
    act(cx, window, |_, _| {});
    let before = editor.read_with(cx, |editor, _| editor.scroll_top());
    act(cx, window, |window, cx| window.input("2", cx));
    for _ in 0..3 {
        act(cx, window, |_, _| {});
    }
    editor.read_with(cx, |editor, _| {
        assert!(editor.text().contains("| 12  |"), "the cell was edited");
        assert_eq!(editor.scroll_top(), before);
    });
}

#[gpui_kit::test]
fn sentence_focus_stops_at_a_list_item(cx: &mut TestAppContext) {
    let (window, editor) = open_editor(cx, "- one two\n- three four");
    set_settings(cx, |s| s.focus_unit = crate::settings::FocusUnit::Sentence);
    editor.update(cx, |editor, cx| editor.move_to(15, cx));
    act(cx, window, |window, cx| window.press("cmd-d", cx));
    editor.read_with(cx, |editor, _| {
        assert_eq!(editor.focus_range(), Some(12..22));
    });
}

fn open_folder(
    cx: &mut TestAppContext,
    root: &std::path::Path,
) -> (AnyWindowHandle, Entity<Workspace>) {
    let root = root.to_path_buf();
    cx.update(|cx| {
        gpui_kit::init(cx);
        editor::bind_keys(cx);
        crate::workspace::bind_keys(cx);
        crate::switcher::bind_keys(cx);
        cx.set_global(Settings::default());
        let bounds = Bounds {
            origin: point(px(0.), px(0.)),
            size: size(px(900.), px(700.)),
        };
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            ..WindowOptions::default()
        };
        gpui_kit::open_window(options, cx, |window, cx| {
            let workspace = cx.new(|cx| Workspace::new_folder(root, window, cx));
            Workspace::focus_editor(&workspace, window, cx);
            workspace
        })
        .expect("open test window")
    })
}

/// A folder with `old.md` and a newer `new.md`.
fn notes_folder(name: &str, new: &str) -> std::path::PathBuf {
    let root = crate::folder::tests::temp_folder(name);
    std::fs::write(root.join("old.md"), "old text").unwrap();
    std::fs::write(root.join("new.md"), new).unwrap();
    let past = std::time::SystemTime::now() - std::time::Duration::from_hours(1);
    std::fs::File::options()
        .write(true)
        .open(root.join("old.md"))
        .unwrap()
        .set_modified(past)
        .unwrap();
    root
}

fn current_title(cx: &mut TestAppContext, workspace: &Entity<Workspace>) -> String {
    workspace.read_with(cx, |w, cx| w.editor().read(cx).title())
}

#[gpui_kit::test]
fn a_folder_opens_its_newest_file(cx: &mut TestAppContext) {
    let root = notes_folder("newest", "new text");
    let (_, workspace) = open_folder(cx, &root);
    assert_eq!(current_title(cx, &workspace), "new.md");
}

#[gpui_kit::test]
fn an_empty_folder_opens_untitled(cx: &mut TestAppContext) {
    let root = crate::folder::tests::temp_folder("empty");
    let (_, workspace) = open_folder(cx, &root);
    assert_eq!(current_title(cx, &workspace), "Untitled.md");
}

#[gpui_kit::test]
fn the_sidebar_opens_files_and_saves_the_current_one(cx: &mut TestAppContext) {
    let root = notes_folder("sidebar", "new text");
    let (window, workspace) = open_folder(cx, &root);
    act(cx, window, |window, _| {
        assert!(window.try_find("sidebar").is_none(), "collapsed");
    });
    act(cx, window, |window, cx| {
        window.press("cmd-down", cx);
        window.input("!", cx);
        window.press("ctrl-cmd-s", cx);
    });
    act(cx, window, |window, cx| window.click(("file", 1usize), cx));
    assert_eq!(current_title(cx, &workspace), "old.md");
    assert_eq!(
        std::fs::read_to_string(root.join("new.md")).unwrap(),
        "new text!"
    );
}

#[gpui_kit::test]
fn switching_files_writes_an_open_cell_into_its_own_file(cx: &mut TestAppContext) {
    let root = notes_folder("cell", "| a |\n|---|\n| 1 |\n");
    let (window, workspace) = open_folder(cx, &root);
    act(cx, window, |window, cx| click_cell(window, cx, 0, 1, 0));
    act(cx, window, |window, cx| window.input("Z", cx));
    act(cx, window, |window, cx| {
        window.dispatch_action(Box::new(crate::workspace::ToggleSidebar), cx);
    });
    act(cx, window, |window, cx| window.click(("file", 1usize), cx));
    assert_eq!(current_title(cx, &workspace), "old.md");
    assert!(
        std::fs::read_to_string(root.join("new.md"))
            .unwrap()
            .contains("1Z")
    );
    assert_eq!(
        std::fs::read_to_string(root.join("old.md")).unwrap(),
        "old text"
    );
}

#[gpui_kit::test]
fn focus_mode_hides_the_sidebar(cx: &mut TestAppContext) {
    let root = notes_folder("focus-sidebar", "text");
    let (window, _) = open_folder(cx, &root);
    act(cx, window, |window, cx| window.press("ctrl-cmd-s", cx));
    act(cx, window, |window, _| {
        assert!(window.try_find("sidebar").is_some());
    });
    act(cx, window, |window, cx| window.press("cmd-d", cx));
    act(cx, window, |window, _| {
        assert!(window.try_find("sidebar").is_none());
    });
}

fn switcher_open(window: &mut Window) -> bool {
    window.try_find("switcher").is_some()
}

#[gpui_kit::test]
fn the_quick_switcher_finds_and_opens_a_file(cx: &mut TestAppContext) {
    let root = notes_folder("switcher", "new text");
    std::fs::write(root.join("design.md"), "design text").unwrap();
    let (window, workspace) = open_folder(cx, &root);
    act(cx, window, |window, cx| window.press("cmd-p", cx));
    act(cx, window, |window, _| assert!(switcher_open(window)));
    act(cx, window, |window, cx| window.input("dsg", cx));
    act(cx, window, |_, _| {});
    act(cx, window, |window, _| {
        let first = window.find(("switch-result", 0usize));
        assert_eq!(first.label(), Some("design.md"));
    });
    act(cx, window, |window, cx| window.press("enter", cx));
    act(cx, window, |window, _| assert!(!switcher_open(window)));
    assert_eq!(current_title(cx, &workspace), "design.md");
}

#[gpui_kit::test]
fn arrows_choose_and_escape_closes_the_switcher(cx: &mut TestAppContext) {
    let root = notes_folder("switcher-keys", "new text");
    let (window, workspace) = open_folder(cx, &root);
    act(cx, window, |window, cx| window.press("cmd-p", cx));
    act(cx, window, |window, cx| window.press("down", cx));
    act(cx, window, |window, cx| window.press("enter", cx));
    // With no query the newest file comes first, so down picks the older one.
    assert_eq!(current_title(cx, &workspace), "old.md");
    act(cx, window, |window, cx| window.press("cmd-p", cx));
    act(cx, window, |window, cx| window.press("escape", cx));
    act(cx, window, |window, cx| {
        assert!(!switcher_open(window));
        let editor = workspace.read(cx).editor().clone();
        assert!(
            editor.read(cx).focus_handle.is_focused(window),
            "the editor has focus again"
        );
    });
}

#[gpui_kit::test]
fn focus_mode_dims_tables_outside_the_caret_paragraph(cx: &mut TestAppContext) {
    let (window, editor) = open_editor(cx, "text\n\n| a |\n|---|\n| 1 |\n");
    editor.read_with(cx, |editor, _| assert!(!editor.table_dimmed(0)));
    act(cx, window, |window, cx| window.press("cmd-d", cx));
    editor.read_with(cx, |editor, _| assert!(editor.table_dimmed(0)));
}

#[gpui_kit::test]
fn a_larger_text_size_makes_lines_taller(cx: &mut TestAppContext) {
    let (window, editor) = open_editor(cx, "text");
    act(cx, window, |_, _| {});
    let medium = editor.read_with(cx, |e, _| e.head_row_bounds().unwrap().size.height);
    set_settings(cx, |s| s.text_size = crate::settings::TextSize::Huge);
    act(cx, window, |_, _| {});
    act(cx, window, |_, _| {});
    let huge = editor.read_with(cx, |e, _| e.head_row_bounds().unwrap().size.height);
    assert!(huge > medium + px(4.), "{medium:?} grew to {huge:?}");
}

#[gpui_kit::test]
fn a_folder_whose_newest_file_is_latin1_opens_it(cx: &mut TestAppContext) {
    let root = crate::folder::tests::temp_folder("latin1");
    std::fs::write(root.join("latin1.md"), b"caf\xe9").unwrap();
    let (_, workspace) = open_folder(cx, &root);
    assert_eq!(current_title(cx, &workspace), "latin1.md");
    workspace.read_with(cx, |w, cx| {
        let editor = w.editor().read(cx);
        assert_eq!(editor.text(), "café");
        assert!(editor.bar_state().file_name.contains("Windows-1252"));
    });
}

#[gpui_kit::test]
fn a_folder_whose_newest_file_cannot_be_read_opens_a_savable_untitled(cx: &mut TestAppContext) {
    use std::os::unix::fs::PermissionsExt as _;
    let root = crate::folder::tests::temp_folder("unreadable");
    let file = root.join("locked.md");
    std::fs::write(&file, "secret").unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o000)).unwrap();
    let (window, workspace) = open_folder(cx, &root);
    assert_eq!(current_title(cx, &workspace), "Untitled.md");
    act(cx, window, |window, _| {
        let banner = window.find("error-banner");
        let label = banner.label().unwrap_or_default();
        assert!(
            label.starts_with("Could not open") && label.contains("locked.md"),
            "{label}"
        );
    });
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
}

#[gpui_kit::test]
fn requests_open_a_window_per_file_and_reuse_open_ones(cx: &mut TestAppContext) {
    use crate::instance::Request;
    let root = notes_folder("windows", "new text");
    cx.update(|cx| {
        gpui_kit::init(cx);
        editor::bind_keys(cx);
        cx.set_global(Settings::default());
        crate::windows::init(cx);
    });
    let open = |cx: &mut TestAppContext, path: std::path::PathBuf| {
        cx.update(|cx| {
            let request = Request {
                path: Some(path),
                ..Request::default()
            };
            crate::windows::open(request, None, cx);
        });
    };
    open(cx, root.join("new.md"));
    open(cx, root.join("old.md"));
    open(cx, root.join("new.md"));
    assert_eq!(
        cx.update(|cx| cx.windows().len()),
        2,
        "an open file is not opened twice"
    );
    open(cx, root.clone());
    assert_eq!(
        cx.update(|cx| cx.windows().len()),
        3,
        "a folder opens its own window"
    );
}

#[gpui_kit::test]
fn without_a_bundle_there_is_no_updater(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.set_global(Settings::default());
        crate::updates::init(cx);
        assert!(!crate::updates::available(cx));
        assert_eq!(crate::menus::update_item(cx).0, "Focal Releases…");
    });
}

/// A folder whose newest file, `notes.md`, holds `text`, next to `other.md`.
fn linked_folder(name: &str, text: &str) -> std::path::PathBuf {
    let root = crate::folder::tests::temp_folder(name);
    std::fs::write(root.join("other.md"), "other text").unwrap();
    let past = std::time::SystemTime::now() - std::time::Duration::from_hours(1);
    std::fs::File::options()
        .write(true)
        .open(root.join("other.md"))
        .unwrap()
        .set_modified(past)
        .unwrap();
    std::fs::write(root.join("notes.md"), text).unwrap();
    root
}

fn editor_of(cx: &mut TestAppContext, workspace: &Entity<Workspace>) -> Entity<Editor> {
    workspace.read_with(cx, |w, _| w.editor().clone())
}

#[gpui_kit::test]
fn a_wiki_link_opens_its_file_and_back_returns(cx: &mut TestAppContext) {
    let root = linked_folder("wiki", "See [[Other]] now.");
    let (window, workspace) = open_folder(cx, &root);
    let editor = editor_of(cx, &workspace);
    editor.update(cx, |e, cx| e.move_to(7, cx));
    act(cx, window, |window, cx| window.press("cmd-enter", cx));
    assert_eq!(current_title(cx, &workspace), "other.md");
    act(cx, window, |window, cx| window.press("cmd-[", cx));
    assert_eq!(current_title(cx, &workspace), "notes.md");
    let editor = editor_of(cx, &workspace);
    editor.read_with(cx, |e, _| {
        assert_eq!(e.selection(), 7..7, "the caret is back");
    });
    act(cx, window, |window, cx| window.press("cmd-[", cx));
    assert_eq!(
        current_title(cx, &workspace),
        "notes.md",
        "nothing more to go back to"
    );
}

#[gpui_kit::test]
fn an_unresolved_wiki_link_opens_a_new_file(cx: &mut TestAppContext) {
    let root = linked_folder("wiki-new", "Plan [[Next Steps|later]].");
    let (window, workspace) = open_folder(cx, &root);
    editor_of(cx, &workspace).update(cx, |e, cx| e.move_to(8, cx));
    act(cx, window, |window, cx| window.press("cmd-enter", cx));
    assert_eq!(current_title(cx, &workspace), "Next Steps.md");
    let editor = editor_of(cx, &workspace);
    editor.read_with(cx, |e, _| {
        assert_eq!(e.path(), Some(root.join("Next Steps.md").as_path()));
    });
}

#[gpui_kit::test]
fn an_anchor_link_moves_to_its_heading(cx: &mut TestAppContext) {
    let (window, editor) = open_editor(cx, "[go](#next-steps)\n\n## Next Steps\n");
    editor.update(cx, |e, cx| e.move_to(2, cx));
    act(cx, window, |window, cx| window.press("cmd-enter", cx));
    editor.read_with(cx, |e, _| assert_eq!(e.selection(), 22..22));
}

const NOTES: &str = "One[^a] and two[^b].\n\n[^a]: The *first* note.\n";

#[gpui_kit::test]
fn a_footnote_reference_jumps_to_its_note_and_back(cx: &mut TestAppContext) {
    let (window, editor) = open_editor(cx, NOTES);
    editor.update(cx, |e, cx| e.move_to(4, cx));
    act(cx, window, |window, cx| window.press("cmd-enter", cx));
    editor.read_with(cx, |e, _| {
        assert_eq!(e.selection(), 28..28, "at the note's text");
    });
    act(cx, window, |window, cx| window.press("cmd-enter", cx));
    editor.read_with(cx, |e, _| {
        assert_eq!(e.selection(), 28..28, "inside the note, not on its label");
    });
    editor.update(cx, |e, cx| e.move_to(23, cx));
    act(cx, window, |window, cx| window.press("cmd-enter", cx));
    editor.read_with(cx, |e, _| {
        assert_eq!(e.selection(), 3..3, "the label leads to the reference");
    });
    act(cx, window, |window, cx| window.press("cmd-[", cx));
    act(cx, window, |window, cx| window.press("cmd-[", cx));
    editor.read_with(cx, |e, _| assert_eq!(e.selection(), 4..4, "back twice"));
}

#[gpui_kit::test]
fn hovering_a_footnote_previews_it(cx: &mut TestAppContext) {
    let (window, editor) = open_editor(cx, NOTES);
    editor.update(cx, |e, cx| e.preview_footnote_at(Some(4), cx));
    act(cx, window, |window, _| {
        let preview = window.find("footnote-preview");
        assert_eq!(preview.label(), Some("The first note."));
    });
    editor.update(cx, |e, cx| e.preview_footnote_at(Some(16), cx));
    act(cx, window, |window, _| {
        assert!(
            window.try_find("footnote-preview").is_none(),
            "b has no note"
        );
    });
    editor.update(cx, |e, cx| e.move_to(16, cx));
    act(cx, window, |window, cx| window.press("cmd-enter", cx));
    editor.read_with(cx, |e, _| {
        assert_eq!(e.selection(), 16..16, "no jump without a note");
    });
}

const MATTER: &str = "---\ntitle: x\n---\n\nBody\n";

#[gpui_kit::test]
fn front_matter_is_collapsed_with_the_caret_in_the_body(cx: &mut TestAppContext) {
    let (window, editor) = open_editor(cx, MATTER);
    editor.read_with(cx, |e, _| assert_eq!(e.selection(), 18..18, "at the body"));
    act(cx, window, |window, _| {
        assert!(window.try_find("front-matter").is_some());
    });
    act(cx, window, |window, cx| {
        window.press("up", cx);
        window.press("up", cx);
    });
    editor.read_with(cx, |e, _| {
        assert_eq!(e.selection(), 13..13, "on its last line");
    });
    act(cx, window, |window, _| {
        assert!(
            window.try_find("front-matter").is_none(),
            "the source shows"
        );
    });
}

#[gpui_kit::test]
fn clicking_front_matter_shows_its_source(cx: &mut TestAppContext) {
    let (window, editor) = open_editor(cx, MATTER);
    act(cx, window, |window, cx| window.click("front-matter", cx));
    editor.read_with(cx, |e, _| {
        assert_eq!(e.selection(), 4..4, "at the first field");
    });
    act(cx, window, |window, _| {
        assert!(window.try_find("front-matter").is_none());
    });
}

/// A 1×1 transparent PNG.
const PIXEL: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
    0x42, 0x60, 0x82,
];

fn image_folder(name: &str, text: &str) -> std::path::PathBuf {
    let root = crate::folder::tests::temp_folder(name);
    std::fs::write(root.join("pixel.png"), PIXEL).unwrap();
    std::fs::write(root.join("notes.md"), text).unwrap();
    root
}

#[gpui_kit::test]
fn an_image_line_shows_the_image(cx: &mut TestAppContext) {
    let root = image_folder("image", "Above\n\n![A pixel](pixel.png)\n");
    let (window, workspace) = open_folder(cx, &root);
    act(cx, window, |window, _| {
        assert!(
            window.try_find(("image-island", 2usize)).is_some(),
            "drawn in place of the line"
        );
    });
    editor_of(cx, &workspace).update(cx, |e, cx| e.move_to(9, cx));
    act(cx, window, |window, _| {
        assert!(
            window.try_find(("image-island", 2usize)).is_none(),
            "the source shows"
        );
        assert!(
            window.try_find(("image-preview", 2usize)).is_some(),
            "with the image below"
        );
    });
}

#[gpui_kit::test]
fn a_missing_image_says_so(cx: &mut TestAppContext) {
    let root = image_folder("image-missing", "![Gone](nowhere.png)\n");
    let (window, _) = open_folder(cx, &root);
    act(cx, window, |window, _| {
        let note = window.find(("image-missing", 0usize));
        assert_eq!(note.label(), Some("Image not found: nowhere.png"));
    });
}

#[gpui_kit::test]
fn display_math_is_an_island_with_a_preview_while_editing(cx: &mut TestAppContext) {
    let (window, editor) = open_editor(cx, "Before\n\n$$\nE = mc^2\n$$\n");
    act(cx, window, |window, _| {
        assert!(window.try_find(("math-island", 2usize)).is_some());
    });
    editor.update(cx, |e, cx| e.move_to(12, cx));
    act(cx, window, |window, _| {
        assert!(
            window.try_find(("math-island", 2usize)).is_none(),
            "the source shows"
        );
        assert!(
            window.try_find(("math-preview", 2usize)).is_some(),
            "with a preview"
        );
    });
}

#[gpui_kit::test]
fn a_mermaid_block_is_a_diagram_with_a_preview_while_editing(cx: &mut TestAppContext) {
    let (window, editor) = open_editor(cx, "Intro\n\n```mermaid\nflowchart TD\n  A --> B\n```\n");
    act(cx, window, |window, _| {
        assert!(window.try_find(("diagram-island", 2usize)).is_some());
    });
    editor.update(cx, |e, cx| e.move_to(20, cx));
    act(cx, window, |window, _| {
        assert!(
            window.try_find(("diagram-island", 2usize)).is_none(),
            "the source shows"
        );
        assert!(
            window.try_find(("diagram-preview", 2usize)).is_some(),
            "with a preview"
        );
    });
}

// ---- Find and replace ----------------------------------------------------------

fn find(cx: &mut TestAppContext, editor: &Entity<Editor>, query: &str) {
    let query = Some(query.to_owned());
    editor.update(cx, |e, cx| e.set_query(query, cx));
}

#[gpui_kit::test]
fn searching_selects_the_first_match_after_the_caret(cx: &mut TestAppContext) {
    let (_, editor) = open_editor(cx, "cat one\nCat two\ncat three");
    editor.update(cx, |e, cx| e.move_to(3, cx));
    find(cx, &editor, "cat");
    editor.read_with(cx, |e, _| {
        assert_eq!(e.selection(), 8..11);
        assert_eq!(e.find_status(), Some((Some(1), 3)));
    });
}

#[gpui_kit::test]
fn next_and_previous_wrap_around(cx: &mut TestAppContext) {
    let (_, editor) = open_editor(cx, "cat one\ncat two");
    find(cx, &editor, "cat");
    editor.update(cx, |e, cx| e.find_step(true, cx));
    editor.read_with(cx, |e, _| assert_eq!(e.selection(), 8..11));
    editor.update(cx, |e, cx| e.find_step(true, cx));
    editor.read_with(cx, |e, _| {
        assert_eq!(e.selection(), 0..3, "wraps to the first");
    });
    editor.update(cx, |e, cx| e.find_step(false, cx));
    editor.read_with(cx, |e, _| {
        assert_eq!(e.selection(), 8..11, "wraps to the last");
    });
}

#[gpui_kit::test]
fn matches_follow_edits(cx: &mut TestAppContext) {
    let (window, editor) = open_editor(cx, "cat one");
    find(cx, &editor, "cat");
    act(cx, window, |window, cx| {
        window.press("cmd-down", cx);
        window.input(" cat", cx);
    });
    editor.read_with(cx, |e, _| assert_eq!(e.find_status(), Some((None, 2))));
}

#[gpui_kit::test]
fn replace_with_the_caret_elsewhere_only_moves_to_a_match(cx: &mut TestAppContext) {
    let (_, editor) = open_editor(cx, "a cat and a cat");
    find(cx, &editor, "cat");
    editor.update(cx, |e, cx| e.move_to(5, cx));
    editor.update(cx, |e, cx| e.replace_current("dog", cx));
    editor.read_with(cx, |e, _| {
        assert_eq!(e.text(), "a cat and a cat", "nothing replaced");
        assert_eq!(e.selection(), 12..15);
    });
    editor.update(cx, |e, cx| e.replace_current("dog", cx));
    editor.read_with(cx, |e, _| {
        assert_eq!(e.text(), "a cat and a dog");
        assert_eq!(e.selection(), 2..5, "the next match is selected");
    });
}

#[gpui_kit::test]
fn replace_all_is_one_undo_step(cx: &mut TestAppContext) {
    let (window, editor) = open_editor(cx, "cat, Cat, cat");
    find(cx, &editor, "cat");
    editor.update(cx, |e, cx| e.replace_all("dog", cx));
    editor.read_with(cx, |e, _| {
        assert_eq!(e.text(), "dog, dog, dog");
        assert_eq!(e.find_status(), Some((None, 0)));
    });
    act(cx, window, |window, cx| window.press("cmd-z", cx));
    editor.read_with(cx, |e, _| assert_eq!(e.text(), "cat, Cat, cat"));
}

#[gpui_kit::test]
fn the_find_bar_searches_as_you_type_and_return_finds_the_next(cx: &mut TestAppContext) {
    let (window, editor) = open_editor(cx, "cat one\ncat two\ncat three");
    act(cx, window, |window, cx| window.press("cmd-f", cx));
    act(cx, window, |window, _| {
        assert!(window.try_find("find-bar").is_some());
    });
    act(cx, window, |window, cx| window.input("cat", cx));
    act(cx, window, |_, _| {});
    editor.read_with(cx, |e, _| assert_eq!(e.selection(), 0..3));
    act(cx, window, |window, cx| window.press("enter", cx));
    editor.read_with(cx, |e, _| assert_eq!(e.selection(), 8..11));
    act(cx, window, |window, cx| window.press("shift-enter", cx));
    editor.read_with(cx, |e, _| assert_eq!(e.selection(), 0..3));
}

#[gpui_kit::test]
fn escape_closes_the_find_bar_and_find_next_still_works(cx: &mut TestAppContext) {
    let (window, editor) = open_editor(cx, "one two one two");
    act(cx, window, |window, cx| window.press("cmd-f", cx));
    act(cx, window, |window, cx| window.input("two", cx));
    act(cx, window, |_, _| {});
    act(cx, window, |window, cx| window.press("escape", cx));
    act(cx, window, |window, cx| {
        assert!(window.try_find("find-bar").is_none());
        assert!(
            editor.read(cx).focus_handle.is_focused(window),
            "the editor has focus"
        );
    });
    editor.read_with(cx, |e, _| {
        assert_eq!(e.selection(), 4..7, "the match stays selected");
        assert_eq!(e.find_status(), None, "matches are no longer highlighted");
    });
    act(cx, window, |window, cx| window.press("cmd-g", cx));
    editor.read_with(cx, |e, _| assert_eq!(e.selection(), 12..15));
}

#[gpui_kit::test]
fn the_find_bar_starts_with_the_selected_text(cx: &mut TestAppContext) {
    let (window, editor) = open_editor(cx, "one two one two");
    editor.update(cx, |e, cx| e.select(4..7, cx));
    act(cx, window, |window, cx| window.press("cmd-f", cx));
    act(cx, window, |window, cx| window.press("enter", cx));
    editor.read_with(cx, |e, _| assert_eq!(e.selection(), 12..15));
}

#[gpui_kit::test]
fn replace_all_through_the_find_bar(cx: &mut TestAppContext) {
    let (window, editor) = open_editor(cx, "cat, Cat, cat");
    act(cx, window, |window, cx| window.press("cmd-alt-f", cx));
    act(cx, window, |window, cx| window.input("cat", cx));
    act(cx, window, |window, cx| window.click("replace-field", cx));
    act(cx, window, |window, cx| window.input("dog", cx));
    act(cx, window, |window, cx| window.click("replace-all", cx));
    editor.read_with(cx, |e, _| assert_eq!(e.text(), "dog, dog, dog"));
}

// ---- New, Open and closing untitled text ---------------------------------------

#[gpui_kit::test]
fn new_opens_an_untitled_window(cx: &mut TestAppContext) {
    let (window, _) = open_workspace(cx, "text");
    cx.update(|cx| {
        crate::windows::init(cx);
        crate::windows::bind_keys(cx);
    });
    act(cx, window, |window, cx| window.press("cmd-n", cx));
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| cx.windows().len()), 2);
}

#[gpui_kit::test]
fn edited_untitled_text_is_not_closed_without_asking(cx: &mut TestAppContext) {
    let (window, editor) = open_editor(cx, "");
    editor.read_with(cx, |e, _| assert!(!e.asks_before_closing()));
    act(cx, window, |window, cx| window.input("draft", cx));
    editor.read_with(cx, |e, _| assert!(e.asks_before_closing()));
    act(cx, window, |window, cx| window.press("cmd-w", cx));
    cx.run_until_parked();
    assert_eq!(
        cx.update(|cx| cx.windows().len()),
        1,
        "the window stays until the question is answered"
    );
}

#[gpui_kit::test]
fn emptied_untitled_text_closes_without_asking(cx: &mut TestAppContext) {
    let (window, editor) = open_editor(cx, "");
    act(cx, window, |window, cx| {
        window.input("x", cx);
        window.press("backspace", cx);
    });
    editor.read_with(cx, |e, _| assert!(!e.asks_before_closing()));
}

#[gpui_kit::test]
fn edits_that_conflict_with_the_disk_are_not_closed_without_asking(cx: &mut TestAppContext) {
    let dir = std::env::temp_dir().join(format!("focal-conflict-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("note.md");
    std::fs::write(&path, "mine").unwrap();
    let (document, text) = Document::open(path.clone()).unwrap();
    let (window, workspace) = open_document(cx, document, &text);
    let editor = workspace.read_with(cx, |w, _| w.editor().clone());
    act(cx, window, |window, cx| {
        window.press("cmd-down", cx);
        window.input(" and more", cx);
    });
    // Another program writes the file before Focal saved the edit.
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(&path, "theirs").unwrap();
    editor.update(cx, |e, cx| e.check_disk(cx));
    editor.read_with(cx, |e, _| assert!(e.asks_before_closing()));
    act(cx, window, |window, cx| window.press("cmd-w", cx));
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| cx.windows().len()), 1, "the window stays");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "theirs");
}

// ---- Go to Heading -------------------------------------------------------------

#[gpui_kit::test]
fn go_to_heading_moves_to_the_chosen_heading_and_back_returns(cx: &mut TestAppContext) {
    let (window, editor) = open_editor(cx, "# Intro\n\ntext\n\n## Details\n\nmore\n\n## Summary\n");
    cx.update(crate::switcher::bind_keys);
    act(cx, window, |window, cx| window.press("cmd-shift-o", cx));
    act(cx, window, |window, _| assert!(switcher_open(window)));
    act(cx, window, |window, cx| window.input("summ", cx));
    act(cx, window, |_, _| {});
    act(cx, window, |window, cx| window.press("enter", cx));
    act(cx, window, |window, _| assert!(!switcher_open(window)));
    editor.read_with(cx, |e, _| assert_eq!(e.selection(), 36..36, "at the title"));
    act(cx, window, |window, cx| window.press("cmd-[", cx));
    editor.read_with(cx, |e, _| {
        assert_eq!(e.selection(), 0..0, "back where it was");
    });
}

#[gpui_kit::test]
fn go_to_heading_without_headings_does_nothing(cx: &mut TestAppContext) {
    let (window, editor) = open_editor(cx, "no headings here");
    cx.update(crate::switcher::bind_keys);
    act(cx, window, |window, cx| window.press("cmd-shift-o", cx));
    act(cx, window, |window, _| assert!(switcher_open(window)));
    act(cx, window, |window, cx| window.press("enter", cx));
    editor.read_with(cx, |e, _| assert_eq!(e.selection(), 0..0));
}

#[gpui_kit::test]
fn matches_in_table_cells_are_highlighted(cx: &mut TestAppContext) {
    let (_, editor) = open_editor(
        cx,
        "Intro\n\n| Pet | Sound |\n|---|---|\n| cat | meow |\n| dog | woof cat |\n",
    );
    find(cx, &editor, "cat");
    editor.read_with(cx, |e, _| {
        let table = &e.snapshot.analysis.tables[0];
        let cell = table.rows[1][0].clone();
        let line = e.snapshot.analysis.lines.line_of(cell.start);
        let view = focal_core::range_view(&e.snapshot.analysis, e.text(), line, cell.clone(), None);
        assert_eq!(
            e.cell_highlights(&cell, &view),
            [(0..3, true)],
            "the current match"
        );
        let other = table.rows[2][1].clone();
        let line = e.snapshot.analysis.lines.line_of(other.start);
        let view =
            focal_core::range_view(&e.snapshot.analysis, e.text(), line, other.clone(), None);
        assert_eq!(e.cell_highlights(&other, &view), [(5..8, false)]);
    });
    editor.update(cx, |e, cx| e.find_step(true, cx));
    editor.read_with(cx, |e, _| {
        let cell = e.snapshot.analysis.tables[0].rows[2][1].clone();
        let line = e.snapshot.analysis.lines.line_of(cell.start);
        let view = focal_core::range_view(&e.snapshot.analysis, e.text(), line, cell.clone(), None);
        assert_eq!(
            e.cell_highlights(&cell, &view),
            [(5..8, true)],
            "the next match"
        );
    });
}

// ---- Export and copy as HTML ---------------------------------------------------

#[gpui_kit::test]
fn export_as_html_writes_a_page_where_chosen(cx: &mut TestAppContext) {
    let dir = std::env::temp_dir().join(format!("focal-export-ui-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let page = dir.join("out.html");
    let _ = std::fs::remove_file(&page);
    let (window, _) = open_editor(cx, "# Hello\n\nSome *text* and $x^2$.\n");
    act(cx, window, |window, cx| window.press("cmd-shift-e", cx));
    cx.simulate_new_path_selection(|_| Some(page.clone()));
    cx.run_until_parked();
    let html = std::fs::read_to_string(&page).expect("the page was written");
    assert!(
        html.contains("<title>Hello</title>"),
        "named after its heading"
    );
    assert!(html.contains("<em>text</em>") && html.contains(r#"<span class="math"><svg"#));
}

#[gpui_kit::test]
fn copy_as_html_copies_the_selection_as_html_and_markdown(cx: &mut TestAppContext) {
    let (window, editor) = open_editor(cx, "Keep **this** part. Not this.");
    editor.update(cx, |e, cx| e.select(0..19, cx));
    act(cx, window, |window, cx| window.press("cmd-alt-shift-c", cx));
    cx.run_until_parked();
    let (html, plain) = crate::mac::copied_html().expect("something was copied");
    assert!(html.contains("<strong>this</strong>"), "{html}");
    assert!(!html.contains("Not this"), "{html}");
    assert_eq!(plain, "Keep **this** part.");
}

// ---- Drafts ----------------------------------------------------------------------

fn drafts_folder(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("focal-drafts-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn drafts_in(dir: &std::path::Path) -> Vec<String> {
    let mut texts: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| std::fs::read_to_string(entry.path()).unwrap())
        .collect();
    texts.sort();
    texts
}

#[gpui_kit::test]
fn untitled_text_is_kept_as_a_draft_until_saved(cx: &mut TestAppContext) {
    let dir = drafts_folder("kept");
    let (window, _) = open_editor(cx, "");
    cx.update(|cx| cx.set_global(crate::drafts::Drafts::in_dir(dir.clone())));
    act(cx, window, |window, cx| window.input("a draft", cx));
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    assert_eq!(drafts_in(&dir), ["a draft"]);

    let saved = dir.with_extension("saved.md");
    act(cx, window, |window, cx| window.press("cmd-s", cx));
    cx.simulate_new_path_selection(|_| Some(saved.clone()));
    cx.run_until_parked();
    assert_eq!(std::fs::read_to_string(&saved).unwrap(), "a draft");
    assert!(drafts_in(&dir).is_empty(), "the draft goes once saved");
}

#[gpui_kit::test]
fn emptied_untitled_text_leaves_no_draft(cx: &mut TestAppContext) {
    let dir = drafts_folder("emptied");
    let (window, _) = open_editor(cx, "");
    cx.update(|cx| cx.set_global(crate::drafts::Drafts::in_dir(dir.clone())));
    act(cx, window, |window, cx| window.input("x", cx));
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    act(cx, window, |window, cx| window.press("backspace", cx));
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    assert!(drafts_in(&dir).is_empty());
}

#[gpui_kit::test]
fn drafts_reopen_as_unsaved_untitled_documents(cx: &mut TestAppContext) {
    let dir = drafts_folder("restore");
    std::fs::write(dir.join("Untitled 1.md"), "left over").unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        editor::bind_keys(cx);
        cx.set_global(Settings::default());
        crate::windows::init(cx);
        cx.set_global(crate::drafts::Drafts::in_dir(dir.clone()));
        crate::drafts::restore(cx);
    });
    cx.run_until_parked();
    let workspaces = cx.update(|cx| crate::windows::workspaces(cx));
    assert_eq!(workspaces.len(), 1);
    workspaces[0].read_with(cx, |w, cx| {
        let editor = w.editor().read(cx);
        assert_eq!(editor.text(), "left over");
        assert!(editor.asks_before_closing(), "a restored draft is unsaved");
    });
}

// ---- Dialects ------------------------------------------------------------------

#[gpui_kit::test]
fn bracket_and_fenced_math_are_islands(cx: &mut TestAppContext) {
    let (window, _) = open_editor(cx, "Intro\n\n\\[\nE = mc^2\n\\]\n\n```math\nx = 1\n```\n");
    act(cx, window, |window, _| {
        assert!(
            window.try_find(("math-island", 2usize)).is_some(),
            "\\[ … \\]"
        );
        assert!(
            window.try_find(("math-island", 6usize)).is_some(),
            "```math"
        );
    });
}

#[gpui_kit::test]
fn an_obsidian_image_embed_is_found_anywhere_in_the_folder(cx: &mut TestAppContext) {
    let root = crate::folder::tests::temp_folder("embed");
    std::fs::create_dir_all(root.join("attachments")).unwrap();
    std::fs::write(root.join("attachments/pixel.png"), PIXEL).unwrap();
    std::fs::write(root.join("notes.md"), "Above\n\n![[pixel.png|40]]\n").unwrap();
    let (window, _) = open_folder(cx, &root);
    act(cx, window, |window, _| {
        assert!(
            window.try_find(("image-island", 2usize)).is_some(),
            "found in attachments/"
        );
    });
}

#[gpui_kit::test]
fn a_table_of_contents_lists_the_headings_and_jumps(cx: &mut TestAppContext) {
    let (window, editor) = open_editor(cx, "# Guide\n\n[TOC]\n\n## First\n\n## Second\n");
    act(cx, window, |window, _| {
        assert!(window.try_find(("toc-island", 2usize)).is_some());
    });
    act(cx, window, |window, cx| {
        window.click(("toc-entry", 2usize), cx);
    });
    editor.read_with(cx, |e, _| assert_eq!(e.selection(), 29..29, "at “Second”"));
    act(cx, window, |window, cx| window.press("cmd-[", cx));
    editor.read_with(cx, |e, _| assert_eq!(e.selection(), 0..0, "back"));
}

#[gpui_kit::test]
fn graphviz_svgbob_and_pikchr_blocks_are_diagrams(cx: &mut TestAppContext) {
    let text = "Intro\n\n```dot\ndigraph { a -> b }\n```\n\n```bob\n+--+\n|a |\n+--+\n```\n\n```pikchr\nbox \"x\"\n```\n";
    let (window, _) = open_editor(cx, text);
    act(cx, window, |window, _| {
        for line in [2usize, 6, 12] {
            assert!(
                window.try_find(("diagram-island", line)).is_some(),
                "line {line}"
            );
        }
    });
}

// ---- Folding -------------------------------------------------------------------

fn shows_line(cx: &mut TestAppContext, editor: &Entity<Editor>, line: usize) -> bool {
    editor.read_with(cx, |e, _| {
        e.snapshot.rows.contains(&editor::Row::Line(line))
    })
}

#[gpui_kit::test]
fn a_folded_callout_opens_and_closes_from_its_title(cx: &mut TestAppContext) {
    let (window, editor) = open_editor(cx, "Intro\n\n> [!faq]- Why?\n> Hidden answer\n\nAfter\n");
    assert!(!shows_line(cx, &editor, 3), "folded as written");
    act(cx, window, |window, cx| {
        window.click(("fold-toggle", 2usize), cx);
    });
    assert!(shows_line(cx, &editor, 3), "opened");
    act(cx, window, |window, cx| {
        window.click(("fold-toggle", 2usize), cx);
    });
    assert!(!shows_line(cx, &editor, 3), "closed again");
}

#[gpui_kit::test]
fn the_caret_entering_a_folded_body_shows_it(cx: &mut TestAppContext) {
    let text = "Intro\n\n<details>\n<summary>More</summary>\n\nHidden\n\n</details>\n";
    let (_, editor) = open_editor(cx, text);
    assert!(!shows_line(cx, &editor, 5));
    let hidden = text.find("Hidden").unwrap();
    editor.update(cx, |e, cx| e.move_to(hidden, cx));
    assert!(shows_line(cx, &editor, 5), "the caret's line shows");
}

#[gpui_kit::test]
fn down_from_a_folded_title_skips_its_body(cx: &mut TestAppContext) {
    let text = "> [!faq]- Why?\n> Hidden one\n> Hidden two\n\nAfter\n";
    let (window, editor) = open_editor(cx, text);
    act(cx, window, |window, cx| window.press("down", cx));
    act(cx, window, |_, _| {});
    editor.read_with(cx, |e, _| {
        let line = e.snapshot.analysis.lines.line_of(e.selection().start);
        assert!(line >= 3, "past the body, on line {line}");
    });
    assert!(!shows_line(cx, &editor, 1), "still folded");
    // And back up from below lands on the title, not in the body.
    let after = text.find("After").unwrap();
    editor.update(cx, |e, cx| e.move_to(after, cx));
    act(cx, window, |window, cx| {
        window.press("up", cx);
        window.press("up", cx);
    });
    editor.read_with(cx, |e, _| {
        assert_eq!(e.snapshot.analysis.lines.line_of(e.selection().start), 0);
    });
    assert!(!shows_line(cx, &editor, 1), "still folded");
}

#[gpui_kit::test]
fn folds_opened_by_hand_stay_open_when_the_file_opens_again(cx: &mut TestAppContext) {
    let dir = std::env::temp_dir().join(format!("focal-fold-memory-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("faq.md");
    std::fs::write(&path, "> [!faq]- Why?\n> Because.\n").unwrap();
    let (document, text) = Document::open(path.clone()).unwrap();
    let (window, workspace) = open_document(cx, document, &text);
    cx.update(|cx| cx.set_global(crate::fold_memory::FoldMemory::default()));
    let editor = workspace.read_with(cx, |w, _| w.editor().clone());
    act(cx, window, |window, cx| {
        window.click(("fold-toggle", 0usize), cx);
    });
    assert!(shows_line(cx, &editor, 1));
    let (document, text) = Document::open(path).unwrap();
    let (_, again) = open_document(cx, document, &text);
    let editor = again.read_with(cx, |w, _| w.editor().clone());
    assert!(shows_line(cx, &editor, 1), "opened as it was left");
}

#[gpui_kit::test]
fn an_html_table_draws_as_a_grid_until_the_caret_enters(cx: &mut TestAppContext) {
    let text = "Intro\n\n<table>\n<tr><th>A</th><th>B</th></tr>\n<tr><td>1</td><td>2</td></tr>\n</table>\n";
    let (window, editor) = open_editor(cx, text);
    act(cx, window, |window, _| {
        assert!(window.try_find(("html-table", 2usize)).is_some());
    });
    let inside = text.find("<tr>").unwrap();
    editor.update(cx, |e, cx| e.move_to(inside, cx));
    act(cx, window, |window, _| {
        assert!(
            window.try_find(("html-table", 2usize)).is_none(),
            "the source shows"
        );
    });
}

#[gpui_kit::test]
fn grammar_issues_are_found_in_prose_and_can_be_turned_off(cx: &mut TestAppContext) {
    let (_, editor) = open_editor(cx, "I has a apple.\n\n`I has a apple.`\n");
    editor.update(cx, |editor, _| {
        let issues = editor.grammar_issues(0, false);
        assert!(
            issues
                .iter()
                .any(|issue| issue.corrections.contains(&"an".to_owned())),
            "{issues:?}"
        );
        assert!(
            editor.grammar_issues(2, false).is_empty(),
            "code is not prose"
        );
        editor.check_grammar = false;
        assert!(editor.grammar_issues(0, false).is_empty());
    });
}

/// Types `typed` at the end of `text` with automatic correction on or off.
fn type_with_correction(cx: &mut TestAppContext, text: &str, typed: &str, on: bool) -> String {
    let (window, editor) = open_editor(cx, text);
    editor.update(cx, |editor, _| editor.correct_spelling = on);
    act(cx, window, |window, cx| {
        window.press("cmd-down", cx);
        for ch in typed.chars() {
            window.input(&ch.to_string(), cx);
        }
    });
    editor.read_with(cx, |editor, _| editor.text().to_owned())
}

#[gpui_kit::test]
fn a_misspelled_word_is_corrected_when_it_is_finished(cx: &mut TestAppContext) {
    let (window, editor) = open_editor(cx, "I saw");
    editor.update(cx, |editor, _| editor.correct_spelling = true);
    act(cx, window, |window, cx| {
        window.press("cmd-down", cx);
        for ch in " teh ".chars() {
            window.input(&ch.to_string(), cx);
        }
    });
    editor.read_with(cx, |editor, _| {
        assert_eq!(editor.text(), "I saw the ");
        assert_eq!(editor.selection(), 10..10);
    });
    act(cx, window, |window, cx| {
        window.press("cmd-z", cx);
    });
    editor.read_with(cx, |editor, _| {
        assert_eq!(
            editor.text(),
            "I saw teh ",
            "undo takes the correction back"
        );
    });
}

#[gpui_kit::test]
fn words_are_left_alone_without_correction_in_code_and_urls(cx: &mut TestAppContext) {
    assert_eq!(
        type_with_correction(cx, "I saw", " teh ", false),
        "I saw teh "
    );
    assert_eq!(
        type_with_correction(cx, "```\nlet", " teh ", true),
        "```\nlet teh "
    );
    assert_eq!(
        type_with_correction(cx, "See https://example.com/teh", ".", true),
        "See https://example.com/teh."
    );
    assert_eq!(
        type_with_correction(cx, "I saw", " Teh,", true),
        "I saw The,"
    );
}
