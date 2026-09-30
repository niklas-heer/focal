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
    let text = text.to_owned();
    cx.update(|cx| {
        gpui_kit::init(cx);
        editor::bind_keys(cx);
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
            let workspace = cx.new(|cx| Workspace::new(Document::untitled(), text, window, cx));
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
