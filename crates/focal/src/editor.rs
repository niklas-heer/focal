//! The editor view: one virtualized list row per source line (or per table
//! island), each drawn as styled text with its Markdown markers hidden or
//! replaced away from the caret.

use std::cell::RefCell;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::Range;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use focal_core::analysis::{InlineStyle, LineKind};
use focal_core::display::{Run, mark_runs, prose_ranges};
use focal_core::{
    Analysis, Bias, Buffer, Caret, EditKind, LineView, analyze, editing, line_view, range_view,
};
use gpui_kit::accesskit::{ActionData, TextSelection};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::native_menu::NativeMenu;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AccessibleAction, HighlightStyle, Role, StatefulInteractiveElement as _, TestSupportExt as _,
    WindowAppearance,
};
use gpui_kit::{
    App, Bounds, ClipboardItem, Context, CursorStyle, ElementInputHandler, EntityInputHandler,
    FocusHandle, Focusable, FontStyle, FontWeight, Hsla, InteractiveElement as _, IntoElement,
    KeyBinding, ListAlignment, ListState, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, ParentElement as _, Pixels, Point, Render, StrikethroughStyle, Styled as _,
    StyledText, Task, TextLayout, TextRun, UTF16Selection, UnderlineStyle, Window,
    WindowControlArea, actions, canvas, div, fill, font, list, point, px, relative, size,
};

use crate::accessibility::{A11yDocument, A11ySource, RunIds};
use crate::document::{self, Document, Stamp};
use crate::spell::SpellChecker;
use crate::theme::{BOLD_PROSE_FONT, MONO_FONT, PROSE_FONT, Theme};

actions!(
    focal,
    [
        Backspace,
        Delete,
        DeleteWordBack,
        DeleteToLineStart,
        Left,
        Right,
        Up,
        Down,
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        WordLeft,
        WordRight,
        SelectWordLeft,
        SelectWordRight,
        LineStart,
        LineEnd,
        SelectLineStart,
        SelectLineEnd,
        DocStart,
        DocEnd,
        SelectDocStart,
        SelectDocEnd,
        SelectAll,
        Newline,
        PlainNewline,
        Indent,
        Outdent,
        Copy,
        Cut,
        Paste,
        Undo,
        Redo,
        Save,
        Bold,
        Italic,
        ToggleFocusMode,
        CloseWindow,
        Quit,
        ShowCharacterPalette,
        CellExit,
    ]
);

/// Replaces a misspelled word, chosen from the spelling menu.
#[derive(Clone, Debug, PartialEq, gpui_kit::Action)]
#[action(namespace = focal, no_json)]
pub struct ReplaceWord {
    range: Range<usize>,
    word: String,
    replacement: String,
}

#[derive(Clone, Debug, PartialEq, gpui_kit::Action)]
#[action(namespace = focal, no_json)]
pub struct IgnoreSpelling {
    word: String,
}

#[derive(Clone, Debug, PartialEq, gpui_kit::Action)]
#[action(namespace = focal, no_json)]
pub struct LearnSpelling {
    word: String,
}

const CONTEXT: &str = "FocalEditor";
/// Key bindings for the input of the table cell being edited.
const CELL_CONTEXT: &str = "FocalCell > Input";
/// Code blocks whose highlights are kept.
const HIGHLIGHT_CACHE_LIMIT: usize = 2_000;
/// Highlight styles per line of a code block.
type BlockHighlights = Vec<Vec<(Range<usize>, HighlightStyle)>>;
/// Block highlights by language, content hash and dark appearance.
type HighlightCache = std::collections::HashMap<(&'static str, u64, bool), Rc<BlockHighlights>>;
/// Distinct line texts whose spelling results are kept.
const SPELL_CACHE_LIMIT: usize = 20_000;
pub(crate) const TEXT_SIZE: f32 = 18.;
const COLUMN_WIDTH: f32 = 720.;
const AUTOSAVE_DELAY: Duration = Duration::from_millis(400);
/// Editors and agents write in bursts; wait this long before reloading.
const RELOAD_SETTLE: Duration = Duration::from_millis(50);

pub fn bind_keys(cx: &mut App) {
    let context = Some(CONTEXT);
    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, context),
        KeyBinding::new("shift-backspace", Backspace, context),
        KeyBinding::new("delete", Delete, context),
        KeyBinding::new("alt-backspace", DeleteWordBack, context),
        KeyBinding::new("cmd-backspace", DeleteToLineStart, context),
        KeyBinding::new("left", Left, context),
        KeyBinding::new("right", Right, context),
        KeyBinding::new("up", Up, context),
        KeyBinding::new("down", Down, context),
        KeyBinding::new("shift-left", SelectLeft, context),
        KeyBinding::new("shift-right", SelectRight, context),
        KeyBinding::new("shift-up", SelectUp, context),
        KeyBinding::new("shift-down", SelectDown, context),
        KeyBinding::new("alt-left", WordLeft, context),
        KeyBinding::new("alt-right", WordRight, context),
        KeyBinding::new("alt-shift-left", SelectWordLeft, context),
        KeyBinding::new("alt-shift-right", SelectWordRight, context),
        KeyBinding::new("cmd-left", LineStart, context),
        KeyBinding::new("cmd-right", LineEnd, context),
        KeyBinding::new("home", LineStart, context),
        KeyBinding::new("end", LineEnd, context),
        KeyBinding::new("cmd-shift-left", SelectLineStart, context),
        KeyBinding::new("cmd-shift-right", SelectLineEnd, context),
        KeyBinding::new("cmd-up", DocStart, context),
        KeyBinding::new("cmd-down", DocEnd, context),
        KeyBinding::new("cmd-shift-up", SelectDocStart, context),
        KeyBinding::new("cmd-shift-down", SelectDocEnd, context),
        KeyBinding::new("cmd-a", SelectAll, context),
        KeyBinding::new("enter", Newline, context),
        KeyBinding::new("shift-enter", PlainNewline, context),
        KeyBinding::new("tab", Indent, context),
        KeyBinding::new("shift-tab", Outdent, context),
        KeyBinding::new("cmd-c", Copy, context),
        KeyBinding::new("cmd-x", Cut, context),
        KeyBinding::new("cmd-v", Paste, context),
        KeyBinding::new("cmd-z", Undo, context),
        KeyBinding::new("cmd-shift-z", Redo, context),
        KeyBinding::new("cmd-s", Save, context),
        KeyBinding::new("cmd-b", Bold, context),
        KeyBinding::new("cmd-i", Italic, context),
        KeyBinding::new("cmd-d", ToggleFocusMode, context),
        KeyBinding::new("cmd-w", CloseWindow, context),
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("ctrl-cmd-space", ShowCharacterPalette, context),
        KeyBinding::new("escape", CellExit, Some(CELL_CONTEXT)),
    ]);
}

/// A list row: one source line, or a whole table shown as a grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Row {
    Line(usize),
    Table(usize),
}

/// Everything derived from the text and caret, rebuilt after each change.
#[derive(Default)]
pub(crate) struct Snapshot {
    version: Option<u64>,
    pub(crate) analysis: Rc<Analysis>,
    views: Rc<[Rc<LineView>]>,
    rows: Rc<[Row]>,
    /// The source text of this version, for the accessibility tree.
    text: Rc<str>,
    /// Rendered cell texts per table, for the accessibility tree.
    table_cells: Rc<[Vec<Vec<String>>]>,
    /// The accessibility text runs, built only when an assistive app asks.
    a11y_document: Rc<std::cell::OnceCell<A11yDocument>>,
    line_rows: Vec<usize>,
    keys: Vec<u64>,
    /// Lines outside the caret's paragraph, dimmed in focus mode.
    focus: Option<Range<usize>>,
}

/// Where a row was drawn in the last frame, for hit testing and caret moves.
#[derive(Clone)]
pub(crate) struct PaintedRow {
    pub(crate) row: Row,
    pub(crate) layout: Option<TextLayout>,
    pub(crate) view: Option<Rc<LineView>>,
    pub(crate) bounds: Bounds<Pixels>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Granularity {
    Character,
    Word,
    Line,
}

enum Hit {
    Text { offset: usize },
    Table(usize),
}

#[allow(clippy::struct_excessive_bools)]
pub struct Editor {
    pub(crate) focus_handle: FocusHandle,
    buffer: Buffer,
    document: Document,
    pub(crate) snapshot: Snapshot,
    pub(crate) selection: Range<usize>,
    reversed: bool,
    marked: Option<Range<usize>>,
    goal_x: Option<Pixels>,
    selecting: Option<(Granularity, Range<usize>)>,
    list: ListState,
    pub(crate) painted: Rc<RefCell<Vec<PaintedRow>>>,
    focus_mode: bool,
    conflict: bool,
    error: Option<String>,
    trace: bool,
    spell: RefCell<SpellChecker>,
    a11y_ids: RunIds,
    /// Misspelled display ranges per line text.
    spell_cache: RefCell<std::collections::HashMap<String, Rc<[Range<usize>]>>>,
    /// Syntax highlights per code block, by language, content hash and appearance.
    highlights: RefCell<HighlightCache>,
    save_task: Option<Task<()>>,
    /// Watches the file for changes on disk; dropping it stops watching.
    watch: Option<(notify::RecommendedWatcher, Task<()>)>,
    /// The table cell being edited, if any.
    pub(crate) grid: Option<crate::grid::GridSession>,
    pub(crate) next_grid_session: u64,
}

impl Editor {
    pub fn new(
        document: Document,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let weak = cx.entity().downgrade();
        window.on_window_should_close(cx, move |_, cx| {
            weak.update(cx, |this, cx| this.save_now(cx)).ok();
            true
        });
        cx.observe_window_appearance(window, |_, _, cx| cx.notify())
            .detach();
        let mut editor = Self {
            focus_handle: cx.focus_handle(),
            buffer: Buffer::new(text),
            document,
            snapshot: Snapshot::default(),
            selection: 0..0,
            reversed: false,
            marked: None,
            goal_x: None,
            selecting: None,
            list: ListState::new(0, ListAlignment::Top, px(600.)),
            painted: Rc::default(),
            focus_mode: false,
            conflict: false,
            error: None,
            trace: std::env::var_os("FOCAL_TRACE").is_some(),
            spell: RefCell::new(SpellChecker::new()),
            a11y_ids: RunIds::default(),
            spell_cache: RefCell::default(),
            highlights: RefCell::default(),
            save_task: None,
            watch: None,
            grid: None,
            next_grid_session: 0,
        };
        editor.refresh();
        editor.watch_document(cx);
        editor
    }

    pub fn title(&self) -> String {
        self.document.title()
    }

    #[cfg(test)]
    pub(crate) fn selection(&self) -> Range<usize> {
        self.selection.clone()
    }

    pub(crate) fn text(&self) -> &str {
        self.buffer.text()
    }

    const fn head(&self) -> usize {
        if self.reversed {
            self.selection.start
        } else {
            self.selection.end
        }
    }

    const fn tail(&self) -> usize {
        if self.reversed {
            self.selection.end
        } else {
            self.selection.start
        }
    }

    fn dirty(&self) -> bool {
        self.buffer.version() != self.document.saved_version
    }

    // ---- Snapshot ------------------------------------------------------

    /// Rebuilds analysis, line views and rows, and tells the list which rows
    /// changed so the others keep their measured heights and scroll position.
    fn refresh(&mut self) {
        let started = Instant::now();
        let version = self.buffer.version();
        let analysis = if self.snapshot.version == Some(version) {
            std::mem::take(&mut self.snapshot.analysis)
        } else {
            Rc::new(analyze(self.buffer.text()))
        };
        let analyzed = started.elapsed();
        let text = self.buffer.text();
        // While dragging, reveal only around the pointer; revealing the whole
        // selection would reflow the text under it.
        let head = self.head();
        let revealing = if self.selecting.is_some() {
            head..head
        } else {
            self.selection.clone()
        };
        let caret = Caret::new(&analysis, revealing, head);
        let views: Vec<Rc<LineView>> = (0..analysis.line_count())
            .map(|line| Rc::new(line_view(&analysis, text, line, Some(&caret))))
            .collect();

        let mut rows = Vec::with_capacity(views.len());
        let mut line_rows = Vec::with_capacity(views.len());
        let mut line = 0;
        while line < views.len() {
            if let LineKind::Table(table) = analysis.info(line).kind {
                let lines = analysis.tables[table].lines.clone();
                for _ in lines.clone() {
                    line_rows.push(rows.len());
                }
                rows.push(Row::Table(table));
                line = lines.end;
            } else {
                line_rows.push(rows.len());
                rows.push(Row::Line(line));
                line += 1;
            }
        }

        let focus = self
            .focus_mode
            .then(|| paragraph_around(&analysis, text, self.head()));
        let keys: Vec<u64> = rows
            .iter()
            .map(|row| {
                let mut hasher = DefaultHasher::new();
                row.hash(&mut hasher);
                match *row {
                    Row::Line(line) => {
                        views[line].text.hash(&mut hasher);
                        views[line].runs.hash(&mut hasher);
                        analysis.info(line).hash(&mut hasher);
                        focus.as_ref().map(|f| f.contains(&line)).hash(&mut hasher);
                    }
                    Row::Table(table) => {
                        text[analysis.tables[table].range.clone()].hash(&mut hasher);
                    }
                }
                hasher.finish()
            })
            .collect();

        let changed = splice_changed_rows(&self.list, &self.snapshot.keys, &keys);

        if self.trace {
            eprintln!(
                "focal: {} lines, caret {:?}, analyze {:.2} ms, restyle {:.2} ms, {} rows changed",
                views.len(),
                self.selection,
                analyzed.as_secs_f64() * 1000.,
                started.elapsed().saturating_sub(analyzed).as_secs_f64() * 1000.,
                changed,
            );
        }
        let table_cells = rendered_table_cells(&analysis, text);
        // The accessibility nodes depend on the text and the rows only, not on
        // the caret, so keep them across caret moves.
        let a11y_document =
            if self.snapshot.version == Some(version) && self.snapshot.rows[..] == rows[..] {
                self.snapshot.a11y_document.clone()
            } else {
                Rc::default()
            };
        self.snapshot = Snapshot {
            version: Some(version),
            analysis,
            views: views.into(),
            rows: rows.into(),
            text: text.into(),
            table_cells,
            a11y_document,
            line_rows,
            keys,
            focus,
        };
    }

    fn head_row(&self) -> usize {
        let line = self.snapshot.analysis.lines.line_of(self.head());
        self.snapshot.line_rows.get(line).copied().unwrap_or(0)
    }

    // ---- Selection and edits -------------------------------------------

    pub(crate) fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.move_biased(offset, Bias::Right, cx);
    }

    /// Moves the caret, stepping out of a line's prefix (quote bars, list
    /// markers) in the direction of `bias`.
    fn move_biased(&mut self, offset: usize, bias: Bias, cx: &mut Context<Self>) {
        let offset = self.snapshot.analysis.snap(offset, bias);
        self.selection = offset..offset;
        self.reversed = false;
        self.after_selection(cx);
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.select_biased(offset, Bias::Right, cx);
    }

    fn select_biased(&mut self, offset: usize, bias: Bias, cx: &mut Context<Self>) {
        let offset = self.snapshot.analysis.snap(offset, bias);
        let tail = self.tail();
        self.reversed = offset < tail;
        self.selection = tail.min(offset)..tail.max(offset);
        self.after_selection(cx);
    }

    fn after_selection(&mut self, cx: &mut Context<Self>) {
        self.marked = None;
        self.refresh();
        self.list.scroll_to_reveal_item(self.head_row());
        cx.notify();
    }

    fn edit(
        &mut self,
        range: Range<usize>,
        new: &str,
        selection: Range<usize>,
        kind: EditKind,
        cx: &mut Context<Self>,
    ) {
        if !matches!(kind, EditKind::Grid(_)) {
            self.grid = None;
        }
        self.edit_keeping_grid(range, new, selection, kind, cx);
    }

    /// Edits without ending a table cell edit. While a cell is being edited,
    /// the view does not scroll to the text caret.
    pub(crate) fn edit_keeping_grid(
        &mut self,
        range: Range<usize>,
        new: &str,
        selection: Range<usize>,
        kind: EditKind,
        cx: &mut Context<Self>,
    ) {
        let before = self.selection.clone();
        self.buffer
            .edit(range, new, before, selection.clone(), kind);
        self.selection = selection;
        self.reversed = false;
        self.goal_x = None;
        self.refresh();
        if self.grid.is_none() {
            self.list.scroll_to_reveal_item(self.head_row());
        }
        self.schedule_save(cx);
        cx.notify();
    }

    fn apply(&mut self, change: editing::Change, cx: &mut Context<Self>) {
        self.edit(
            change.range,
            &change.text,
            change.selection,
            EditKind::Other,
            cx,
        );
    }

    fn insert(&mut self, text: &str, kind: EditKind, cx: &mut Context<Self>) {
        let range = self.marked.take().unwrap_or_else(|| self.selection.clone());
        let caret = range.start + text.len();
        self.edit(range, text, caret..caret, kind, cx);
    }

    /// Where the content of the line at `offset` starts, after its prefix.
    fn content_start_at(&self, offset: usize) -> usize {
        let analysis = &self.snapshot.analysis;
        analysis.content_range(analysis.lines.line_of(offset)).start
    }

    /// Applies a change and keeps the selection on the same text.
    fn apply_keeping_selection(&mut self, change: &editing::Change, cx: &mut Context<Self>) {
        let selection = self.shifted_selection(change);
        self.edit(
            change.range.clone(),
            &change.text,
            selection,
            EditKind::Other,
            cx,
        );
    }

    /// The selection moved along with the text around `change`.
    pub(crate) fn shifted_selection(&self, change: &editing::Change) -> Range<usize> {
        let range = &change.range;
        let shift = |at: usize| {
            if at >= range.end {
                at - range.len() + change.text.len()
            } else if at > range.start {
                range.start + change.text.len()
            } else {
                at
            }
        };
        shift(self.selection.start)..shift(self.selection.end)
    }

    fn line_range_at(&self, offset: usize) -> Range<usize> {
        let lines = &self.snapshot.analysis.lines;
        lines.range(lines.line_of(offset))
    }

    // ---- Actions ---------------------------------------------------------

    fn backspace(&mut self, _: &Backspace, _: &mut Window, cx: &mut Context<Self>) {
        if self.selection.is_empty() {
            let head = self.head();
            if let Some(change) = editing::backspace_prefix(&self.snapshot.analysis, head) {
                self.apply(change, cx);
                return;
            }
            let start = self.buffer.previous_grapheme(head);
            if start == head {
                return;
            }
            self.edit(start..head, "", start..start, EditKind::Deleting, cx);
        } else {
            let start = self.selection.start;
            self.edit(
                self.selection.clone(),
                "",
                start..start,
                EditKind::Other,
                cx,
            );
        }
    }

    fn delete(&mut self, _: &Delete, _: &mut Window, cx: &mut Context<Self>) {
        if self.selection.is_empty() {
            let head = self.head();
            let end = self.buffer.next_grapheme(head);
            self.edit(head..end, "", head..head, EditKind::Other, cx);
        } else {
            let start = self.selection.start;
            self.edit(
                self.selection.clone(),
                "",
                start..start,
                EditKind::Other,
                cx,
            );
        }
    }

    fn delete_word_back(&mut self, _: &DeleteWordBack, _: &mut Window, cx: &mut Context<Self>) {
        let head = self.head();
        let start = if self.selection.is_empty() {
            self.buffer.previous_word(head)
        } else {
            self.selection.start
        };
        let end = self.selection.end.max(head);
        self.edit(start..end, "", start..start, EditKind::Other, cx);
    }

    fn delete_to_line_start(
        &mut self,
        _: &DeleteToLineStart,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let head = self.head();
        let start = self.line_range_at(head).start;
        let start = if start == head {
            self.buffer.previous_grapheme(head)
        } else {
            start
        };
        self.edit(start..head, "", start..start, EditKind::Other, cx);
    }

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        if self.selection.is_empty() {
            let to = self.buffer.previous_grapheme(self.head());
            self.move_biased(to, Bias::Left, cx);
        } else {
            self.move_to(self.selection.start, cx);
        }
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        if self.selection.is_empty() {
            let to = self.buffer.next_grapheme(self.head());
            self.move_to(to, cx);
        } else {
            self.move_to(self.selection.end, cx);
        }
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        let to = self.buffer.previous_grapheme(self.head());
        self.select_biased(to, Bias::Left, cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        let to = self.buffer.next_grapheme(self.head());
        self.select_to(to, cx);
    }

    fn up(&mut self, _: &Up, _: &mut Window, cx: &mut Context<Self>) {
        let to = self.vertical(-1);
        self.move_to(to, cx);
    }

    fn down(&mut self, _: &Down, _: &mut Window, cx: &mut Context<Self>) {
        let to = self.vertical(1);
        self.move_to(to, cx);
    }

    fn select_up(&mut self, _: &SelectUp, _: &mut Window, cx: &mut Context<Self>) {
        let to = self.vertical(-1);
        self.select_to(to, cx);
    }

    fn select_down(&mut self, _: &SelectDown, _: &mut Window, cx: &mut Context<Self>) {
        let to = self.vertical(1);
        self.select_to(to, cx);
    }

    fn word_left(&mut self, _: &WordLeft, _: &mut Window, cx: &mut Context<Self>) {
        let to = self.buffer.previous_word(self.head());
        self.move_biased(to, Bias::Left, cx);
    }

    fn word_right(&mut self, _: &WordRight, _: &mut Window, cx: &mut Context<Self>) {
        let to = self.buffer.next_word(self.head());
        self.move_to(to, cx);
    }

    fn select_word_left(&mut self, _: &SelectWordLeft, _: &mut Window, cx: &mut Context<Self>) {
        let to = self.buffer.previous_word(self.head());
        self.select_biased(to, Bias::Left, cx);
    }

    fn select_word_right(&mut self, _: &SelectWordRight, _: &mut Window, cx: &mut Context<Self>) {
        let to = self.buffer.next_word(self.head());
        self.select_to(to, cx);
    }

    fn line_start(&mut self, _: &LineStart, _: &mut Window, cx: &mut Context<Self>) {
        let to = self.content_start_at(self.head());
        self.move_to(to, cx);
    }

    fn line_end(&mut self, _: &LineEnd, _: &mut Window, cx: &mut Context<Self>) {
        let to = self.line_range_at(self.head()).end;
        self.move_to(to, cx);
    }

    fn select_line_start(&mut self, _: &SelectLineStart, _: &mut Window, cx: &mut Context<Self>) {
        let to = self.content_start_at(self.head());
        self.select_to(to, cx);
    }

    fn select_line_end(&mut self, _: &SelectLineEnd, _: &mut Window, cx: &mut Context<Self>) {
        let to = self.line_range_at(self.head()).end;
        self.select_to(to, cx);
    }

    fn doc_start(&mut self, _: &DocStart, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
    }

    fn doc_end(&mut self, _: &DocEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.text().len(), cx);
    }

    fn select_doc_start(&mut self, _: &SelectDocStart, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(0, cx);
    }

    fn select_doc_end(&mut self, _: &SelectDocEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.text().len(), cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.selection = 0..self.text().len();
        self.reversed = false;
        self.after_selection(cx);
    }

    fn newline(&mut self, _: &Newline, _: &mut Window, cx: &mut Context<Self>) {
        let line = self.line_range_at(self.selection.start);
        let ending = self.buffer.line_ending();
        match editing::continue_list(self.text(), &line, &self.selection, ending) {
            Some(change) => self.apply(change, cx),
            None => self.insert(ending, EditKind::Other, cx),
        }
    }

    fn plain_newline(&mut self, _: &PlainNewline, _: &mut Window, cx: &mut Context<Self>) {
        let ending = self.buffer.line_ending();
        self.insert(ending, EditKind::Other, cx);
    }

    fn indent(&mut self, _: &Indent, _: &mut Window, cx: &mut Context<Self>) {
        let lines = &self.snapshot.analysis.lines;
        let line = lines.line_of(self.head());
        match editing::indent_list_item(self.text(), lines, line) {
            Some(change) => self.apply_keeping_selection(&change, cx),
            None => self.insert("\t", EditKind::Typing, cx),
        }
    }

    fn outdent(&mut self, _: &Outdent, _: &mut Window, cx: &mut Context<Self>) {
        let lines = &self.snapshot.analysis.lines;
        let line = lines.line_of(self.head());
        if let Some(change) = editing::outdent_list_item(self.text(), lines, line) {
            self.apply_keeping_selection(&change, cx);
        }
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selection.is_empty() {
            let text = self.text()[self.selection.clone()].to_owned();
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    fn cut(&mut self, _: &Cut, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selection.is_empty() {
            let text = self.text()[self.selection.clone()].to_owned();
            cx.write_to_clipboard(ClipboardItem::new_string(text));
            let start = self.selection.start;
            self.edit(
                self.selection.clone(),
                "",
                start..start,
                EditKind::Other,
                cx,
            );
        }
    }

    fn paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.insert(&text, EditKind::Other, cx);
        }
    }

    fn undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(selection) = self.buffer.undo() {
            self.restore(selection, cx);
        }
    }

    fn redo(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(selection) = self.buffer.redo() {
            self.restore(selection, cx);
        }
    }

    fn restore(&mut self, selection: Range<usize>, cx: &mut Context<Self>) {
        self.grid = None;
        let len = self.text().len();
        self.selection = selection.start.min(len)..selection.end.min(len);
        self.reversed = false;
        self.refresh();
        self.list.scroll_to_reveal_item(self.head_row());
        self.schedule_save(cx);
        cx.notify();
    }

    fn bold(&mut self, _: &Bold, _: &mut Window, cx: &mut Context<Self>) {
        let change = editing::toggle_wrap(self.text(), &self.selection, "**");
        self.apply(change, cx);
    }

    fn italic(&mut self, _: &Italic, _: &mut Window, cx: &mut Context<Self>) {
        let change = editing::toggle_wrap(self.text(), &self.selection, "_");
        self.apply(change, cx);
    }

    fn toggle_focus_mode(&mut self, _: &ToggleFocusMode, _: &mut Window, cx: &mut Context<Self>) {
        self.focus_mode = !self.focus_mode;
        self.refresh();
        cx.notify();
    }

    #[allow(clippy::unused_self)]
    fn show_character_palette(
        &mut self,
        _: &ShowCharacterPalette,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        window.show_character_palette();
    }

    fn save(&mut self, _: &Save, window: &mut Window, cx: &mut Context<Self>) {
        if self.document.path.is_some() {
            self.conflict = false;
            self.save_now(cx);
            return;
        }
        let directory = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let chosen = cx.prompt_for_new_path(&directory, Some("Untitled.md"));
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(path))) = chosen.await {
                this.update_in(cx, |this, window, cx| {
                    this.document.path = Some(path);
                    this.watch_document(cx);
                    this.save_now(cx);
                    window.set_window_title(&this.title());
                })
                .ok();
            }
        })
        .detach();
    }

    fn close_window(&mut self, _: &CloseWindow, window: &mut Window, cx: &mut Context<Self>) {
        self.save_now(cx);
        window.remove_window();
    }

    fn quit(&mut self, _: &Quit, _: &mut Window, cx: &mut Context<Self>) {
        self.save_now(cx);
        cx.quit();
    }

    // ---- Files -----------------------------------------------------------

    fn schedule_save(&mut self, cx: &mut Context<Self>) {
        if self.document.path.is_none() || self.conflict {
            return;
        }
        self.save_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(AUTOSAVE_DELAY).await;
            this.update(cx, |this, cx| this.save_now(cx)).ok();
        }));
    }

    fn save_now(&mut self, cx: &mut Context<Self>) {
        self.save_task = None;
        if !self.dirty() || self.conflict {
            return;
        }
        let version = self.buffer.version();
        match self.document.save(self.buffer.text(), version) {
            Ok(()) => self.error = None,
            Err(error) => self.error = Some(format!("{error:#}")),
        }
        cx.notify();
    }

    /// Watches the document's file through FSEvents and reloads it on change.
    fn watch_document(&mut self, cx: &mut Context<Self>) {
        self.watch = None;
        let Some(path) = self.document.path.clone() else {
            return;
        };
        let (watcher, events) = match document::watch(&path) {
            Ok(watch) => watch,
            Err(error) => {
                self.error = Some(format!("Not watching the file for changes: {error:#}"));
                return;
            }
        };
        let task = cx.spawn(async move |this, cx| {
            while events.recv().await.is_ok() {
                cx.background_executor().timer(RELOAD_SETTLE).await;
                while events.try_recv().is_ok() {}
                if this.update(cx, |this, cx| this.check_disk(cx)).is_err() {
                    break;
                }
            }
        });
        self.watch = Some((watcher, task));
    }

    /// Reloads the file when it changed on disk. With unsaved local edits,
    /// it asks instead of choosing a side.
    fn check_disk(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.document.path.clone() else {
            return;
        };
        let stamp = Stamp::of(&path);
        if stamp.is_none() || stamp == self.document.stamp {
            return;
        }
        let Ok(text) = document::read(&path) else {
            return;
        };
        if text == self.text() {
            self.document.stamp = stamp;
            return;
        }
        if self.dirty() {
            self.conflict = true;
            cx.notify();
            return;
        }
        self.load_theirs(&text, stamp, cx);
    }

    fn load_theirs(&mut self, text: &str, stamp: Option<Stamp>, cx: &mut Context<Self>) {
        self.grid = None;
        let len = text.len();
        let selection = self.selection.start.min(len)..self.selection.end.min(len);
        self.buffer.replace_all(text, selection.clone());
        self.selection = selection;
        self.document.stamp = stamp;
        self.document.saved_version = self.buffer.version();
        self.conflict = false;
        self.save_task = None;
        self.refresh();
        cx.notify();
    }

    fn keep_mine(&mut self, cx: &mut Context<Self>) {
        self.conflict = false;
        self.save_now(cx);
    }

    fn reload_from_disk(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.document.path.clone() else {
            return;
        };
        match document::read(&path) {
            Ok(text) => self.load_theirs(&text, Stamp::of(&path), cx),
            Err(error) => {
                self.error = Some(format!("{error:#}"));
                cx.notify();
            }
        }
    }

    // ---- Geometry --------------------------------------------------------

    fn hit_test(&self, position: Point<Pixels>) -> Option<Hit> {
        let painted = self.painted.borrow();
        let row = painted
            .iter()
            .find(|row| row.bounds.top() <= position.y && position.y < row.bounds.bottom())
            .or_else(|| {
                let distance = |row: &PaintedRow| {
                    f32::from(if position.y < row.bounds.top() {
                        row.bounds.top() - position.y
                    } else {
                        position.y - row.bounds.bottom()
                    })
                };
                painted
                    .iter()
                    .min_by(|a, b| distance(a).total_cmp(&distance(b)))
            })?;
        match row.row {
            Row::Table(table) => Some(Hit::Table(table)),
            Row::Line(_) => {
                let (layout, view) = (row.layout.as_ref()?, row.view.as_ref()?);
                let display = display_index(layout, position);
                Some(Hit::Text {
                    offset: view.map.to_source(display),
                })
            }
        }
    }

    /// The source offset one visual line above or below the caret.
    fn vertical(&mut self, direction: i32) -> usize {
        let analysis = &self.snapshot.analysis;
        let head = self.head();
        let line = analysis.lines.line_of(head);
        let painted = self.painted.borrow().clone();
        let find = |line: usize| {
            painted.iter().find_map(|row| match row.row {
                Row::Line(l) if l == line => Some((row.layout.clone()?, row.view.clone()?)),
                _ => None,
            })
        };
        let Some((layout, view)) = find(line) else {
            return self.vertical_by_column(direction);
        };
        let Some(position) = caret_position(&layout, view.map.to_display(head)) else {
            return self.vertical_by_column(direction);
        };
        let x = *self.goal_x.get_or_insert(position.x);
        let line_height = layout.line_height();
        let bounds = layout.bounds();
        for step in 1..=2 {
            let target_y = position.y + line_height * 0.5 + line_height * (direction * step) as f32;
            if target_y < bounds.top() || bounds.bottom() <= target_y {
                break;
            }
            let target = view
                .map
                .to_source(display_index(&layout, point(x, target_y)));
            if target != head {
                return target;
            }
        }
        let target = if direction < 0 {
            match line.checked_sub(1) {
                Some(target) => target,
                None => return 0,
            }
        } else if line + 1 < analysis.line_count() {
            line + 1
        } else {
            return self.text().len();
        };
        let target_row = self
            .snapshot
            .line_rows
            .get(target)
            .and_then(|&row| self.snapshot.rows.get(row));
        if let Some(&Row::Table(table)) = target_row {
            let table = &analysis.tables[table];
            return if direction < 0 {
                analysis.lines.range(table.lines.end - 1).start
            } else {
                table.range.start
            };
        }
        match find(target) {
            Some((layout, view)) => {
                let bounds = layout.bounds();
                let half = layout.line_height() * 0.5;
                let y = if direction < 0 {
                    bounds.bottom() - half
                } else {
                    bounds.top() + half
                };
                view.map.to_source(display_index(&layout, point(x, y)))
            }
            None => self.vertical_by_column(direction),
        }
    }

    fn vertical_by_column(&self, direction: i32) -> usize {
        let lines = &self.snapshot.analysis.lines;
        let head = self.head();
        let line = lines.line_of(head);
        let column = head - lines.range(line).start;
        let target = if direction < 0 {
            match line.checked_sub(1) {
                Some(target) => target,
                None => return 0,
            }
        } else if line + 1 < lines.len() {
            line + 1
        } else {
            return self.text().len();
        };
        let range = lines.range(target);
        let mut offset = (range.start + column).min(range.end);
        while !self.text().is_char_boundary(offset) {
            offset -= 1;
        }
        offset
    }

    // ---- Mouse -----------------------------------------------------------

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus_handle, cx);
        self.goal_x = None;
        let Some(hit) = self.hit_test(event.position) else {
            return;
        };
        let offset = match hit {
            Hit::Table(table) => {
                self.edit_cell(table, 0, 0, window, cx);
                return;
            }
            Hit::Text { offset } => offset,
        };
        self.grid = None;
        if event.modifiers.platform
            && let Some(link) = self.snapshot.analysis.link_at(offset)
        {
            if link.destination.contains("://") || link.destination.starts_with("mailto:") {
                cx.open_url(&link.destination);
            }
            return;
        }
        let granularity = match event.click_count {
            2 => Granularity::Word,
            n if n >= 3 => Granularity::Line,
            _ => Granularity::Character,
        };
        let unit = self.unit_at(offset, granularity);
        self.selecting = Some((granularity, unit.clone()));
        if event.modifiers.shift {
            self.select_to(offset, cx);
        } else {
            self.selection = unit;
            self.reversed = false;
            self.after_selection(cx);
        }
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some((granularity, anchor)) = self.selecting.clone() else {
            return;
        };
        if event.pressed_button != Some(MouseButton::Left) {
            self.selecting = None;
            return;
        }
        let Some(Hit::Text { offset, .. }) = self.hit_test(event.position) else {
            return;
        };
        let unit = self.unit_at(offset, granularity);
        let (start, end) = (anchor.start.min(unit.start), anchor.end.max(unit.end));
        let reversed = unit.start < anchor.start;
        if self.selection != (start..end) || self.reversed != reversed {
            self.selection = start..end;
            self.reversed = reversed;
            self.after_selection(cx);
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.selecting.take().is_some() && !self.selection.is_empty() {
            self.refresh();
            cx.notify();
        }
    }

    fn unit_at(&self, offset: usize, granularity: Granularity) -> Range<usize> {
        match granularity {
            Granularity::Character => offset..offset,
            Granularity::Word => self.buffer.word_at(offset),
            Granularity::Line => {
                let range = self.line_range_at(offset);
                let ending = self.text()[range.end..].find('\n').map_or(0, |ix| ix + 1);
                range.start..range.end + ending
            }
        }
    }

    pub(crate) fn toggle_task_on_line(&mut self, line: usize, cx: &mut Context<Self>) {
        let Some(marker) = self.snapshot.analysis.info(line).prefix.marker.clone() else {
            return;
        };
        let Some(open) = self.text()[marker.clone()].find('[') else {
            return;
        };
        let at = marker.start + open + 1;
        let checked = matches!(self.text().as_bytes().get(at), Some(b'x' | b'X'));
        let selection = self.selection.clone();
        self.edit(
            at..at + 1,
            if checked { " " } else { "x" },
            selection,
            EditKind::Other,
            cx,
        );
    }

    // ---- Accessibility ------------------------------------------------------

    fn a11y_source(&self) -> A11ySource {
        A11ySource {
            analysis: self.snapshot.analysis.clone(),
            text: self.snapshot.text.clone(),
            rows: self.snapshot.rows.clone(),
            table_cells: self.snapshot.table_cells.clone(),
            document: self.snapshot.a11y_document.clone(),
        }
    }

    /// Applies a selection requested by an assistive app such as VoiceOver.
    fn select_from_a11y(&mut self, selection: &TextSelection, cx: &mut Context<Self>) {
        let source = self.a11y_source();
        let (Some(anchor), Some(focus)) = (
            source.source_offset(&self.a11y_ids, &selection.anchor),
            source.source_offset(&self.a11y_ids, &selection.focus),
        ) else {
            return;
        };
        self.selection = anchor.min(focus)..anchor.max(focus);
        self.reversed = focus < anchor;
        self.goal_x = None;
        self.after_selection(cx);
    }

    // ---- Code highlighting ------------------------------------------------

    /// Highlighted runs for a line of a fenced code block in a known language.
    fn code_runs(
        &self,
        line: usize,
        view: &LineView,
        base: Hsla,
        dimmed: bool,
        dark: bool,
    ) -> Option<Vec<TextRun>> {
        let analysis = &self.snapshot.analysis;
        let info = analysis.info(line);
        if info.kind != LineKind::Code {
            return None;
        }
        let block = analysis.code_blocks.get(info.code_block?)?;
        let language = crate::highlight::language_name(block.language.as_deref()?)?;
        let lines: Vec<usize> = block
            .lines
            .clone()
            .filter(|&l| analysis.info(l).kind == LineKind::Code)
            .collect();
        let index = lines.iter().position(|&l| l == line)?;
        let texts: Vec<&str> = lines
            .iter()
            .map(|&l| &self.text()[analysis.content_range(l)])
            .collect();
        let mut hasher = DefaultHasher::new();
        texts.hash(&mut hasher);
        let key = (language, hasher.finish(), dark);
        let highlights = {
            let mut cache = self.highlights.borrow_mut();
            if cache.len() > HIGHLIGHT_CACHE_LIMIT {
                cache.clear();
            }
            cache
                .entry(key)
                .or_insert_with(|| {
                    Rc::new(crate::highlight::highlight_block(language, &texts, dark))
                })
                .clone()
        };
        let styles = highlights.get(index)?;
        let fade = |color: Hsla| if dimmed { color.opacity(0.3) } else { color };
        let run = |len: usize, style: Option<&HighlightStyle>| {
            let mut font = font(MONO_FONT);
            if let Some(style) = style {
                font.weight = style.font_weight.unwrap_or(font.weight);
                font.style = style.font_style.unwrap_or(font.style);
            }
            TextRun {
                len,
                font,
                color: fade(style.and_then(|s| s.color).unwrap_or(base)),
                background_color: None,
                underline: None,
                strikethrough: None,
            }
        };
        let mut runs = Vec::new();
        let mut at = 0;
        for (range, style) in styles {
            let range = range.start.max(at).min(view.text.len())..range.end.min(view.text.len());
            if range.start >= range.end {
                continue;
            }
            if range.start > at {
                runs.push(run(range.start - at, None));
            }
            runs.push(run(range.len(), Some(style)));
            at = range.end;
        }
        if at < view.text.len() {
            runs.push(run(view.text.len() - at, None));
        }
        Some(runs)
    }

    // ---- Spelling -----------------------------------------------------------

    /// Misspelled words of a line, as display ranges. Only prose is checked;
    /// with `hide_at_caret`, the word being typed is left alone.
    fn misspelled(
        &self,
        line: usize,
        view: &LineView,
        kind: &LineKind,
        hide_at_caret: bool,
    ) -> Vec<Range<usize>> {
        if !matches!(kind, LineKind::Text | LineKind::Heading(_)) || view.text.trim().is_empty() {
            return Vec::new();
        }
        let found = {
            let mut cache = self.spell_cache.borrow_mut();
            if cache.len() > SPELL_CACHE_LIMIT {
                cache.clear();
            }
            cache
                .entry(view.text.clone())
                .or_insert_with(|| self.spell.borrow().misspellings(&view.text).into())
                .clone()
        };
        let prose = prose_ranges(view);
        let head = self.head();
        let line_range = self.snapshot.analysis.content_range(line);
        let caret = (hide_at_caret && line_range.start <= head && head <= line_range.end)
            .then(|| view.map.to_display(head));
        found
            .iter()
            .filter(|word| {
                prose
                    .iter()
                    .any(|p| p.start <= word.start && word.end <= p.end)
            })
            .filter(|word| caret.is_none_or(|at| at < word.start || at > word.end))
            .cloned()
            .collect()
    }

    fn on_right_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus_handle, cx);
        let Some(Hit::Text { offset, .. }) = self.hit_test(event.position) else {
            return;
        };
        let line = self.snapshot.analysis.lines.line_of(offset);
        let view = self.snapshot.views[line].clone();
        let kind = self.snapshot.analysis.info(line).kind.clone();
        let at = view.map.to_display(offset);
        let words = self.misspelled(line, &view, &kind, false);
        let Some(word) = words.iter().find(|w| w.start <= at && at <= w.end).cloned() else {
            return;
        };
        let source = view.map.to_source(word.start)..view.map.to_source(word.end);
        let text = self.text()[source.clone()].to_owned();
        self.selection = source.clone();
        self.reversed = false;
        self.after_selection(cx);

        let guesses = self.spell.borrow().guesses(&view.text, word);
        let mut menu = NativeMenu::new();
        for guess in guesses.iter().take(6) {
            menu = menu.menu(
                guess.clone(),
                Box::new(ReplaceWord {
                    range: source.clone(),
                    word: text.clone(),
                    replacement: guess.clone(),
                }),
            );
        }
        if guesses.is_empty() {
            menu = menu.menu_with_disabled(
                "No Guesses Found",
                true,
                Box::new(IgnoreSpelling { word: text.clone() }),
            );
        }
        menu.separator()
            .menu(
                "Ignore Spelling",
                Box::new(IgnoreSpelling { word: text.clone() }),
            )
            .menu("Learn Spelling", Box::new(LearnSpelling { word: text }))
            .show(event.position, window, cx);
    }

    fn replace_word(&mut self, action: &ReplaceWord, _: &mut Window, cx: &mut Context<Self>) {
        if self.text().get(action.range.clone()) != Some(action.word.as_str()) {
            return;
        }
        let end = action.range.start + action.replacement.len();
        self.edit(
            action.range.clone(),
            &action.replacement,
            end..end,
            EditKind::Other,
            cx,
        );
    }

    fn ignore_spelling(&mut self, action: &IgnoreSpelling, _: &mut Window, cx: &mut Context<Self>) {
        self.spell.borrow_mut().ignore(&action.word);
        self.spell_cache.borrow_mut().clear();
        cx.notify();
    }

    fn learn_spelling(&mut self, action: &LearnSpelling, _: &mut Window, cx: &mut Context<Self>) {
        self.spell.borrow().learn(&action.word);
        self.spell_cache.borrow_mut().clear();
        cx.notify();
    }

    // ---- UTF-16 for the platform input handler ------------------------------

    fn offset_to_utf16(&self, offset: usize) -> usize {
        self.text()[..offset.min(self.text().len())]
            .encode_utf16()
            .count()
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf16 = 0;
        for (ix, ch) in self.text().char_indices() {
            if utf16 >= offset {
                return ix;
            }
            utf16 += ch.len_utf16();
        }
        self.text().len()
    }

    fn range_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range.start)..self.offset_from_utf16(range.end)
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    // ---- Rendering ---------------------------------------------------------

    fn render_row(
        &mut self,
        ix: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui_kit::AnyElement {
        let theme = Theme::for_appearance(window.appearance());
        let row = self.snapshot.rows.get(ix).copied().unwrap_or(Row::Line(0));
        let last = ix + 1 == self.snapshot.rows.len();
        let content = match row {
            Row::Line(line) => self.render_line(line, &theme, window, cx),
            Row::Table(table) => self.render_table(table, &theme, cx),
        };
        div()
            .w_full()
            .flex()
            .justify_center()
            .px(px(48.))
            .when(ix == 0, |d| d.pt(px(56.)))
            .when(last, |d| d.pb(px(240.)))
            .child(div().w_full().max_w(px(COLUMN_WIDTH)).child(content))
            .into_any_element()
    }

    #[allow(clippy::too_many_lines)]
    fn render_line(
        &self,
        line: usize,
        theme: &Theme,
        window: &Window,
        cx: &Context<Self>,
    ) -> gpui_kit::AnyElement {
        let analysis = &self.snapshot.analysis;
        let info = analysis.info(line).clone();
        let view = self.snapshot.views[line].clone();
        let dimmed = self
            .snapshot
            .focus
            .as_ref()
            .is_some_and(|focus| !focus.contains(&line));
        let mono = matches!(
            info.kind,
            LineKind::Code
                | LineKind::CodeFence { .. }
                | LineKind::Html
                | LineKind::FrontMatter
                | LineKind::FrontMatterFence
                | LineKind::Table(_)
        );
        let (scale, weight, top) = match info.kind {
            LineKind::Heading(1) => (1.6, FontWeight::BOLD, 18.),
            LineKind::Heading(2) => (1.35, FontWeight::BOLD, 14.),
            LineKind::Heading(3) => (1.15, FontWeight::BOLD, 10.),
            LineKind::Heading(_) => (1.0, FontWeight::BOLD, 6.),
            _ if mono => (0.86, FontWeight::NORMAL, 0.),
            _ => (1.0, FontWeight::NORMAL, 0.),
        };
        let family = if mono { MONO_FONT } else { PROSE_FONT };
        let mut base = if dimmed {
            theme.text.opacity(0.3)
        } else {
            theme.text
        };
        if matches!(
            info.kind,
            LineKind::FrontMatter | LineKind::FrontMatterFence | LineKind::Html
        ) {
            base = theme.marker;
        }
        let misspelled = self.misspelled(line, &view, &info.kind, true);
        let marked = mark_runs(&view.runs, &misspelled, InlineStyle::MISSPELLED);
        let dark = matches!(
            window.appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        );
        let runs = self
            .code_runs(line, &view, base, dimmed, dark)
            .unwrap_or_else(|| {
                text_runs(
                    &marked,
                    family,
                    weight,
                    base,
                    theme,
                    info.alert.map(|a| theme.alert(a)),
                    dimmed,
                )
            });
        let text = StyledText::new(view.text.clone()).with_runs(runs);
        let layout = text.layout().clone();
        let line_range = analysis.content_range(line);
        let code_fence = matches!(info.kind, LineKind::CodeFence { .. });
        let code = matches!(info.kind, LineKind::Code) || code_fence;
        let rule = info.kind == LineKind::ThematicBreak && view.text.is_empty();

        let selection = self.selection.clone();
        let head = self.head();
        let focused = self.focus_handle.clone();
        let painted = self.painted.clone();
        let paint_layout = layout.clone();
        let paint_view = view.clone();
        let selection_color = theme.selection;
        let caret_color = theme.caret;
        let _ = (window, cx);

        let content = div()
            .relative()
            .pt(px(top))
            .font_family(family)
            .text_size(px(TEXT_SIZE * scale))
            .line_height(relative(if matches!(info.kind, LineKind::Heading(_)) {
                1.3
            } else {
                1.6
            }))
            .when(code, |d| d.px(px(14.)).bg(theme.code_background))
            .when(
                matches!(info.kind, LineKind::CodeFence { opening: true }),
                |d| {
                    d.rounded_tl(px(6.))
                        .rounded_tr(px(6.))
                        .text_size(px(TEXT_SIZE * 0.7))
                },
            )
            .when(
                matches!(info.kind, LineKind::CodeFence { opening: false }),
                |d| {
                    d.rounded_bl(px(6.))
                        .rounded_br(px(6.))
                        .text_size(px(TEXT_SIZE * 0.7))
                },
            )
            .child(
                canvas(|_, _, _| {}, {
                    let layout = layout.clone();
                    let view = view.clone();
                    let selection = selection.clone();
                    let line_range = line_range.clone();
                    move |_, (), window, _| {
                        paint_selection(
                            &layout,
                            &view,
                            &selection,
                            &line_range,
                            selection_color,
                            window,
                        );
                    }
                })
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .when(rule, |d| {
                d.child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .top(px(TEXT_SIZE * 0.8))
                        .h(px(1.))
                        .bg(theme.rule),
                )
            })
            .child(text)
            .child(
                canvas(
                    |_, _, _| {},
                    move |bounds, (), window, _| {
                        painted.borrow_mut().push(PaintedRow {
                            row: Row::Line(line),
                            layout: Some(paint_layout.clone()),
                            view: Some(paint_view.clone()),
                            bounds,
                        });
                        let on_line = line_range.start <= head && head <= line_range.end;
                        if on_line
                            && focused.is_focused(window)
                            && let Some(position) =
                                caret_position(&paint_layout, paint_view.map.to_display(head))
                        {
                            let height = paint_layout.line_height();
                            window.paint_quad(fill(
                                Bounds::new(position, size(px(2.), height)),
                                caret_color,
                            ));
                        }
                    },
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            );
        let entity = cx.entity().downgrade();
        let on_toggle = move |line: usize, _: &mut Window, cx: &mut App| {
            entity
                .update(cx, |editor, cx| editor.toggle_task_on_line(line, cx))
                .ok();
        };
        let line_height = px(TEXT_SIZE
            * scale
            * if matches!(info.kind, LineKind::Heading(_)) {
                1.3
            } else {
                1.6
            });
        crate::prefix::wrap(
            content.into_any_element(),
            &info.prefix,
            line,
            theme,
            line_height,
            on_toggle,
        )
    }

    fn render_banner(&self, theme: &Theme, cx: &mut Context<Self>) -> Option<gpui_kit::AnyElement> {
        if self.conflict {
            let keep = cx.listener(|this, _, _, cx| this.keep_mine(cx));
            let load = cx.listener(|this, _, _, cx| this.reload_from_disk(cx));
            return Some(
                banner(theme)
                    .child("The file changed on disk while you had unsaved edits.")
                    .child(
                        Button::new("keep-mine")
                            .label("Keep mine")
                            .on_click(move |e, w, cx| keep(e, w, cx)),
                    )
                    .child(
                        Button::new("load-theirs")
                            .primary()
                            .label("Load theirs")
                            .on_click(move |e, w, cx| load(e, w, cx)),
                    )
                    .into_any_element(),
            );
        }
        self.error.as_ref().map(|error| {
            banner(theme)
                .child(format!("Could not save: {error}"))
                .into_any_element()
        })
    }
}

fn banner(theme: &Theme) -> gpui_kit::Div {
    div()
        .absolute()
        .bottom(px(16.))
        .left(px(16.))
        .right(px(16.))
        .flex()
        .items_center()
        .gap(px(12.))
        .px(px(16.))
        .py(px(10.))
        .rounded(px(8.))
        .bg(theme.banner)
        .font_family(PROSE_FONT)
        .text_size(px(14.))
}

/// Where the caret for display offset `index` is drawn. GPUI places an offset
/// at a soft-wrap boundary at the end of the earlier visual line; a caret
/// there belongs at the start of the next one.
fn caret_position(layout: &TextLayout, index: usize) -> Option<Point<Pixels>> {
    let position = layout.position_for_index(index)?;
    let wrapped = layout.line_layout_for_index(index)?;
    let at_wrap = wrapped.wrap_boundaries().iter().any(|boundary| {
        wrapped
            .runs()
            .get(boundary.run_ix)
            .and_then(|run| run.glyphs.get(boundary.glyph_ix))
            .is_some_and(|glyph| glyph.index == index)
    });
    let bounds = layout.bounds();
    if at_wrap && position.x > bounds.left() + px(0.5) {
        Some(point(bounds.left(), position.y + layout.line_height()))
    } else {
        Some(position)
    }
}

fn display_index(layout: &TextLayout, position: Point<Pixels>) -> usize {
    let bounds = layout.bounds();
    let x = position
        .x
        .clamp(bounds.left(), bounds.right().max(bounds.left()));
    let y = position
        .y
        .clamp(bounds.top(), (bounds.bottom() - px(1.)).max(bounds.top()));
    match layout.index_for_position(point(x, y)) {
        Ok(ix) | Err(ix) => ix.min(layout.len()),
    }
}

fn paint_selection(
    layout: &TextLayout,
    view: &LineView,
    selection: &Range<usize>,
    line: &Range<usize>,
    color: Hsla,
    window: &mut Window,
) {
    if selection.is_empty() || selection.end < line.start || selection.start > line.end {
        return;
    }
    let start = view.map.to_display(selection.start.max(line.start));
    let end = view.map.to_display(selection.end.min(line.end));
    let (Some(from), Some(to)) = (
        layout.position_for_index(start),
        layout.position_for_index(end),
    ) else {
        return;
    };
    let height = layout.line_height();
    let bounds = layout.bounds();
    let newline = if selection.end > line.end {
        px(7.)
    } else {
        px(0.)
    };
    if from.y == to.y {
        let rect = Bounds::from_corners(from, point(to.x + newline, to.y + height));
        window.paint_quad(fill(rect, color));
        return;
    }
    window.paint_quad(fill(
        Bounds::from_corners(from, point(bounds.right(), from.y + height)),
        color,
    ));
    let middle_top = from.y + height;
    if to.y > middle_top {
        window.paint_quad(fill(
            Bounds::from_corners(
                point(bounds.left(), middle_top),
                point(bounds.right(), to.y),
            ),
            color,
        ));
    }
    window.paint_quad(fill(
        Bounds::from_corners(
            point(bounds.left(), to.y),
            point(to.x + newline, to.y + height),
        ),
        color,
    ));
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn text_runs(
    runs: &[Run],
    family: &str,
    weight: FontWeight,
    base: Hsla,
    theme: &Theme,
    alert: Option<Hsla>,
    dimmed: bool,
) -> Vec<TextRun> {
    let fade = |color: Hsla| if dimmed { color.opacity(0.3) } else { color };
    runs.iter()
        .map(|run| {
            let style = run.style;
            let mono = style.contains(InlineStyle::CODE)
                || style.contains(InlineStyle::MATH)
                || style.contains(InlineStyle::HTML)
                || style.contains(InlineStyle::LABEL);
            let bold = weight == FontWeight::BOLD
                || style.contains(InlineStyle::STRONG)
                || style.contains(InlineStyle::ALERT_TITLE);
            let family = match (mono, bold) {
                (true, _) => MONO_FONT,
                (false, true) if family == PROSE_FONT => BOLD_PROSE_FONT,
                (false, _) => family,
            };
            let mut font = font(family);
            font.weight = if bold {
                FontWeight::BOLD
            } else {
                FontWeight::NORMAL
            };
            if style.contains(InlineStyle::EMPHASIS) || style.contains(InlineStyle::IMAGE) {
                font.style = FontStyle::Italic;
            }
            let color = if style.contains(InlineStyle::MARKER)
                || style.contains(InlineStyle::LABEL)
                || style.contains(InlineStyle::HTML)
                || style.contains(InlineStyle::TASK_DONE)
            {
                theme.marker
            } else if style.contains(InlineStyle::ALERT_TITLE) {
                alert.unwrap_or(base)
            } else if style.contains(InlineStyle::LINK)
                || style.contains(InlineStyle::FOOTNOTE)
                || style.contains(InlineStyle::IMAGE)
                || style.contains(InlineStyle::MATH)
            {
                theme.link
            } else if style.contains(InlineStyle::CODE) {
                theme.code_text
            } else {
                base
            };
            let background = style
                .contains(InlineStyle::HIGHLIGHT)
                .then_some(theme.highlight);
            let strike = style.contains(InlineStyle::STRIKETHROUGH)
                || (style.contains(InlineStyle::TASK_DONE)
                    && !style.contains(InlineStyle::MARKER)
                    && run.len > 4);
            TextRun {
                len: run.len,
                font,
                color: fade(color),
                background_color: background,
                underline: if style.contains(InlineStyle::MISSPELLED) {
                    Some(UnderlineStyle {
                        color: Some(theme.misspelled),
                        thickness: px(1.5),
                        wavy: true,
                    })
                } else {
                    style.contains(InlineStyle::LINK).then(|| UnderlineStyle {
                        color: Some(theme.link.opacity(0.35)),
                        thickness: px(1.),
                        wavy: false,
                    })
                },
                strikethrough: strike.then(|| StrikethroughStyle {
                    color: Some(fade(color)),
                    thickness: px(1.),
                }),
            }
        })
        .collect()
}

/// Tells the list which rows changed between two versions, so unchanged rows
/// keep their measured heights and the scroll position holds. Returns the
/// number of changed rows.
fn splice_changed_rows(list: &ListState, old: &[u64], new: &[u64]) -> usize {
    let prefix = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    let room = old.len().min(new.len()) - prefix;
    let suffix = old
        .iter()
        .rev()
        .zip(new.iter().rev())
        .take(room)
        .take_while(|(a, b)| a == b)
        .count();
    if old.len() != new.len() || prefix != old.len() {
        list.splice(prefix..old.len() - suffix, new.len() - prefix - suffix);
    }
    new.len() - prefix - suffix
}

/// Each table's cells as rendered text, for the accessibility tree.
fn rendered_table_cells(analysis: &Analysis, text: &str) -> Rc<[Vec<Vec<String>>]> {
    analysis
        .tables
        .iter()
        .map(|table| {
            table
                .rows
                .iter()
                .map(|row| {
                    row.iter()
                        .map(|cell| {
                            let line = analysis.lines.line_of(cell.start);
                            range_view(analysis, text, line, cell.clone(), None).text
                        })
                        .collect()
                })
                .collect()
        })
        .collect()
}

/// The lines of the paragraph (non-blank run of lines) around `offset`.
fn paragraph_around(analysis: &Analysis, text: &str, offset: usize) -> Range<usize> {
    let lines = &analysis.lines;
    let blank = |line: usize| text[lines.range(line)].trim().is_empty();
    let line = lines.line_of(offset);
    if blank(line) {
        return line..line + 1;
    }
    let mut start = line;
    while start > 0 && !blank(start - 1) {
        start -= 1;
    }
    let mut end = line + 1;
    while end < lines.len() && !blank(end) {
        end += 1;
    }
    start..end
}

impl EntityInputHandler for Editor {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.text()[range].to_owned())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selection),
            reversed: self.reversed,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked.as_ref().map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.marked = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .map(|range| self.range_from_utf16(&range))
            .or_else(|| self.marked.clone())
            .unwrap_or_else(|| self.selection.clone());
        self.marked = None;
        let caret = range.start + text.len();
        self.edit(range, text, caret..caret, EditKind::Typing, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .map(|range| self.range_from_utf16(&range))
            .or_else(|| self.marked.clone())
            .unwrap_or_else(|| self.selection.clone());
        let within = |utf16: usize| {
            text.char_indices()
                .scan(0, |count, (ix, ch)| {
                    let at = *count;
                    *count += ch.len_utf16();
                    Some((ix, at))
                })
                .find(|&(_, at)| at >= utf16)
                .map_or(text.len(), |(ix, _)| ix)
        };
        let selection = new_selected_range_utf16.map_or_else(
            || range.start + text.len()..range.start + text.len(),
            |sel| range.start + within(sel.start)..range.start + within(sel.end),
        );
        self.edit(range.clone(), text, selection, EditKind::Typing, cx);
        self.marked = (!text.is_empty()).then(|| range.start..range.start + text.len());
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let offset = self.offset_from_utf16(range_utf16.start);
        let line = self.snapshot.analysis.lines.line_of(offset);
        let painted = self.painted.borrow();
        let row = painted.iter().find(|row| row.row == Row::Line(line));
        let Some(row) = row else {
            return Some(element_bounds);
        };
        let (layout, view) = (row.layout.as_ref()?, row.view.as_ref()?);
        let position = layout.position_for_index(view.map.to_display(offset))?;
        Some(Bounds::new(position, size(px(2.), layout.line_height())))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        match self.hit_test(point)? {
            Hit::Text { offset, .. } => Some(self.offset_to_utf16(offset)),
            Hit::Table(_) => None,
        }
    }
}

impl Focusable for Editor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Editor {
    #[allow(clippy::too_many_lines)]
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::for_appearance(window.appearance());
        self.painted.borrow_mut().clear();
        let entity = cx.entity();
        let focus = self.focus_handle.clone();
        let banner = self.render_banner(&theme, cx);
        let title = self.title();
        let a11y = self.a11y_source();
        let a11y_ids = self.a11y_ids.clone();
        let a11y_selection = self.selection.clone();
        let a11y_reversed = self.reversed;
        let a11y_editor = cx.entity().downgrade();
        let a11y_action_source = a11y.clone();
        div()
            .id("focal-editor")
            .role(Role::MultilineTextInput)
            .aria_label(title.clone())
            .a11y_synthetic_children(move |builder| {
                a11y.build_tree(builder, a11y_selection, a11y_reversed, &a11y_ids);
            })
            .on_a11y_action(AccessibleAction::SetTextSelection, move |data, _, cx| {
                if let Some(ActionData::SetTextSelection(selection)) = data {
                    let _ = &a11y_action_source;
                    a11y_editor
                        .update(cx, |editor, cx| editor.select_from_a11y(selection, cx))
                        .ok();
                }
            })
            .key_context(CONTEXT)
            .test_support()
            .track_focus(&self.focus_handle)
            .relative()
            .size_full()
            .bg(theme.background)
            .text_color(theme.text)
            .font_family(PROSE_FONT)
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::delete_word_back))
            .on_action(cx.listener(Self::delete_to_line_start))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::up))
            .on_action(cx.listener(Self::down))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_up))
            .on_action(cx.listener(Self::select_down))
            .on_action(cx.listener(Self::word_left))
            .on_action(cx.listener(Self::word_right))
            .on_action(cx.listener(Self::select_word_left))
            .on_action(cx.listener(Self::select_word_right))
            .on_action(cx.listener(Self::line_start))
            .on_action(cx.listener(Self::line_end))
            .on_action(cx.listener(Self::select_line_start))
            .on_action(cx.listener(Self::select_line_end))
            .on_action(cx.listener(Self::doc_start))
            .on_action(cx.listener(Self::doc_end))
            .on_action(cx.listener(Self::select_doc_start))
            .on_action(cx.listener(Self::select_doc_end))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::newline))
            .on_action(cx.listener(Self::plain_newline))
            .on_action(cx.listener(Self::indent))
            .on_action(cx.listener(Self::outdent))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            .on_action(cx.listener(Self::save))
            .on_action(cx.listener(Self::bold))
            .on_action(cx.listener(Self::italic))
            .on_action(cx.listener(Self::toggle_focus_mode))
            .on_action(cx.listener(Self::show_character_palette))
            .on_action(cx.listener(Self::close_window))
            .on_action(cx.listener(Self::quit))
            .on_action(cx.listener(Self::replace_word))
            .on_action(cx.listener(Self::ignore_spelling))
            .on_action(cx.listener(Self::learn_spelling))
            .on_action(cx.listener(Self::cell_exit))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_down(MouseButton::Right, cx.listener(Self::on_right_mouse_down))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .child(
                canvas(
                    |_, _, _| {},
                    move |bounds, (), window, cx| {
                        window.handle_input(
                            &focus,
                            ElementInputHandler::new(bounds, entity.clone()),
                            cx,
                        );
                    },
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .child(
                list(
                    self.list.clone(),
                    cx.processor(|this, ix: usize, window, cx| this.render_row(ix, window, cx)),
                )
                .size_full(),
            )
            .child(
                div()
                    .id("titlebar")
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .h(px(36.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(12.))
                    .text_color(theme.marker)
                    .bg(theme.background)
                    .window_control_area(WindowControlArea::Drag)
                    .child(title),
            )
            .children(banner)
    }
}
