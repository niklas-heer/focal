//! The editor view: one virtualized list row per source line (or per table
//! island), each drawn as styled text with its Markdown markers hidden or
//! replaced away from the caret.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::Range;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use focal_core::analysis::{InlineStyle, LineKind};
use focal_core::blocks::{block_image, diagram_blocks, math_blocks};
use focal_core::display::{Run, mark_runs, prose_ranges};
use focal_core::text_stats::{reading_minutes, sentence_at, word_count};
use focal_core::{
    Analysis, Bias, Buffer, Caret, EditKind, LineView, analyze, editing, line_view, range_view,
};
use focal_core::{find, links};
use gpui_kit::accesskit::{ActionData, TextSelection};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::native_menu::NativeMenu;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AccessibleAction, EventEmitter, HighlightStyle, Role, SharedString,
    StatefulInteractiveElement as _, TestSupportExt as _, WindowAppearance,
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
use crate::mac::{CloseQuestion, SaveAnswer};
use crate::settings::{FocusUnit, Settings};
use crate::spell::{GrammarIssue, SpellChecker};
use crate::theme::{BOLD_PROSE_FONT, DIMMED, MONO_FONT, PROSE_FONT, Theme, Typography};

mod vim;

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
        CellNext,
        CellPrevious,
        CellBelow,
        CellUp,
        CellDown,
        Strikethrough,
        InlineCode,
        InsertLink,
        ToggleBullets,
        ToggleNumbers,
        ToggleTask,
        ToggleQuote,
        InsertTable,
        InsertCodeBlock,
        InsertMath,
        OpenLink,
        ExportHtml,
        ExportPdf,
        Print,
        CopyHtml,
        Highlight,
        ShowInFinder,
    ]
);

/// Makes the selected lines a heading of this level, or paragraphs with 0.
#[derive(Clone, Debug, PartialEq, gpui_kit::Action)]
#[action(namespace = focal, no_json)]
pub struct SetHeading(pub u8);

/// What the editor asks of its window.
pub enum EditorEvent {
    /// Open this file, from a followed link.
    Open(PathBuf),
    /// The caret jumped (to an anchor or a footnote) from this selection,
    /// which "back" returns to.
    Jumped(Range<usize>),
    /// Search for this text, as the find bar would (Vim's `*` and `#`).
    Search { query: String, forward: bool },
}

impl EventEmitter<EditorEvent> for Editor {}

/// What the bottom bar shows about the document and the caret.
pub(crate) struct BarState {
    pub file_name: SharedString,
    /// The caret line's heading level, 0 for a paragraph.
    pub heading: u8,
    pub words: usize,
    pub selected_words: usize,
    pub minutes: usize,
}

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

pub(crate) const CONTEXT: &str = "FocalEditor";
/// Room below the last line, so the end of a document is not at the
/// window's edge.
const END_PADDING: Pixels = px(240.);
/// How long printing waits for a page and its images to load.
const PRINT_LOAD_LIMIT: Duration = Duration::from_secs(20);
/// How long a finished print keeps its web view, for WebKit to finish.
const PRINT_GRACE: Duration = Duration::from_secs(10);
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
        KeyBinding::new("cmd-shift-h", Highlight, context),
        KeyBinding::new("cmd-alt-r", ShowInFinder, context),
        KeyBinding::new("cmd-shift-x", Strikethrough, context),
        KeyBinding::new("cmd-e", InlineCode, context),
        KeyBinding::new("cmd-k", InsertLink, context),
        KeyBinding::new("cmd-shift-e", ExportHtml, context),
        // ⌘P opens Quick Open, as in code editors.
        KeyBinding::new("cmd-alt-p", Print, context),
        KeyBinding::new("cmd-alt-shift-c", CopyHtml, context),
        KeyBinding::new("cmd-enter", OpenLink, context),
        KeyBinding::new("cmd-0", SetHeading(0), context),
        KeyBinding::new("cmd-1", SetHeading(1), context),
        KeyBinding::new("cmd-2", SetHeading(2), context),
        KeyBinding::new("cmd-3", SetHeading(3), context),
        KeyBinding::new("cmd-4", SetHeading(4), context),
        KeyBinding::new("cmd-5", SetHeading(5), context),
        KeyBinding::new("cmd-6", SetHeading(6), context),
        KeyBinding::new("cmd-d", ToggleFocusMode, context),
        KeyBinding::new("cmd-w", CloseWindow, context),
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("ctrl-cmd-space", ShowCharacterPalette, context),
        KeyBinding::new("escape", CellExit, Some(CELL_CONTEXT)),
        // The document's undo, not the input's: a cell edit rewrites the table.
        KeyBinding::new("cmd-z", Undo, Some(CELL_CONTEXT)),
        KeyBinding::new("cmd-shift-z", Redo, Some(CELL_CONTEXT)),
        KeyBinding::new("tab", CellNext, Some(CELL_CONTEXT)),
        KeyBinding::new("shift-tab", CellPrevious, Some(CELL_CONTEXT)),
        KeyBinding::new("enter", CellBelow, Some(CELL_CONTEXT)),
        KeyBinding::new("up", CellUp, Some(CELL_CONTEXT)),
        KeyBinding::new("down", CellDown, Some(CELL_CONTEXT)),
    ]);
}

/// A list row: one source line, or a whole table shown as a grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Row {
    Line(usize),
    Table(usize),
    Island(Island),
}

/// A block drawn in place of its source lines while the caret is elsewhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Island {
    pub kind: IslandKind,
    /// Its source lines, `start..end`.
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum IslandKind {
    FrontMatter,
    /// An image alone on its line.
    Image,
    /// Display math alone on its lines.
    Math,
    /// A table-of-contents marker such as `[TOC]`.
    Toc,
    /// A table written in HTML.
    HtmlTable,
    /// A fenced Mermaid code block.
    Diagram,
}

/// Everything derived from the text and caret, rebuilt after each change.
#[derive(Default)]
pub(crate) struct Snapshot {
    version: Option<u64>,
    pub(crate) analysis: Rc<Analysis>,
    views: Rc<[Rc<LineView>]>,
    pub(crate) rows: Rc<[Row]>,
    /// The title lines of blocks that fold, with their state.
    folds: Rc<HashMap<usize, FoldTitle>>,
    /// The source text of this version, for the accessibility tree.
    text: Rc<str>,
    /// Rendered cell texts per table, for the accessibility tree.
    table_cells: Rc<[Vec<Vec<String>>]>,
    /// The accessibility text runs, built only when an assistive app asks.
    a11y_document: Rc<std::cell::OnceCell<A11yDocument>>,
    line_rows: Vec<usize>,
    keys: Vec<u64>,
    /// What focus mode keeps bright; everything else is dimmed.
    focus: Option<Focus>,
}

/// A fold's title line as drawn: whether it is folded, the lines it hides,
/// and its key for remembering the state.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct FoldTitle {
    folded: bool,
    body: Range<usize>,
    key: String,
}

/// The caret's paragraph (as lines) and, by sentence, the sentence in it (as
/// source bytes).
#[derive(Clone, Debug, PartialEq, Eq)]
struct Focus {
    lines: Range<usize>,
    sentence: Option<Range<usize>>,
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

/// Where a vertical caret move lands.
enum Vertical {
    To(usize),
    /// On a table, which the caret enters by editing a cell.
    Table(usize),
}

enum Hit {
    Text { offset: usize },
    Table(usize),
    Island(Island),
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
    /// Sideways scroll of each table, by index.
    pub(crate) table_scrolls: RefCell<HashMap<usize, gpui_kit::ScrollHandle>>,
    /// Set when a cell starts being edited, so its table scrolls it into view.
    pub(crate) reveal_cell: Cell<bool>,
    /// Frames left to keep revealing the caret: rows below the viewport are
    /// measured only once laid out, so one reveal can fall short.
    reveal_frames: u8,
    pub(crate) painted: Rc<RefCell<Vec<PaintedRow>>>,
    focus_mode: bool,
    focus_unit: FocusUnit,
    pub(crate) typography: Typography,
    /// Keep the caret's line centered in focus mode.
    typewriter: bool,
    /// Frames left to keep centering the caret's line.
    center_frames: u8,
    /// The viewport height the first and last rows were padded for.
    padded_for: Pixels,
    /// Set when the rows' heights were thrown away and not yet measured.
    remeasured: bool,
    /// The word count of a text version, for the bottom bar.
    word_count: Cell<Option<(u64, usize)>>,
    conflict: bool,
    error: Option<String>,
    trace: bool,
    spell: RefCell<SpellChecker>,
    a11y_ids: RunIds,
    /// Misspelled display ranges per line text.
    spell_cache: RefCell<std::collections::HashMap<String, Rc<[Range<usize>]>>>,
    /// Grammar issues per line text, in display ranges.
    grammar_cache: RefCell<HashMap<String, Rc<[GrammarIssue]>>>,
    pub(crate) check_grammar: bool,
    /// Correct misspelled words as they are finished (also needs the
    /// system's switch).
    pub(crate) correct_spelling: bool,
    /// Syntax highlights per code block, by language, content hash and appearance.
    highlights: RefCell<HighlightCache>,
    save_task: Option<Task<()>>,
    /// Watches the file for changes on disk; dropping it stops watching.
    watch: Option<(notify::RecommendedWatcher, Task<()>)>,
    /// Drawn Mermaid diagrams, by their source and palette.
    pub(crate) diagrams: RefCell<HashMap<crate::islands::DiagramKey, crate::islands::Typeset>>,
    /// The last picture each math or diagram block showed, by kind and line.
    pub(crate) last_pictures: RefCell<HashMap<(&'static str, usize), String>>,
    /// Typeset display math, by its TeX.
    pub(crate) math: RefCell<HashMap<String, crate::islands::Typeset>>,
    /// Remote images being downloaded, and those that failed, by URL.
    pub(crate) fetching_images: RefCell<std::collections::HashSet<String>>,
    pub(crate) failed_images: RefCell<HashMap<String, Instant>>,
    /// Image embeds found in the folder, by name, and when they were looked for.
    pub(crate) found_files: RefCell<HashMap<String, (Option<PathBuf>, Instant)>>,
    /// The pointer's last position, where a footnote preview appears.
    pointer: Point<Pixels>,
    /// The editor's bounds in the window, from the last frame.
    frame: Rc<Cell<Bounds<Pixels>>>,
    /// The footnote reference (by its start) under the pointer, and its note.
    footnote_preview: Option<(usize, SharedString)>,
    /// The folder wiki links resolve in, and its Markdown files relative to it.
    pub(crate) link_root: Option<PathBuf>,
    link_files: std::sync::Arc<[PathBuf]>,
    /// The document being printed, kept until the next print.
    printing: Option<crate::print::Job>,
    /// Where Writing Tools' Replace sends new text for this editor.
    writing_tools: async_channel::Sender<crate::writing_tools::Replacement>,
    /// The table cell being edited, if any.
    pub(crate) grid: Option<crate::grid::GridSession>,
    pub(crate) next_grid_session: u64,
    /// A table shown as Markdown source until the caret leaves it.
    pub(crate) table_source: Option<usize>,
    /// The size, relative to their drawn size, at which math and diagram
    /// SVGs are written. GPUI rasterizes SVG images at twice their size and
    /// shrinks them on the GPU, which drops hairlines (a minus sign in a
    /// superscript); this makes the bitmap match the display's pixels.
    pub(crate) pixel_scale: f32,
    /// The draft file keeping this untitled document's text, if any.
    draft: Option<PathBuf>,
    /// Folds opened or closed by hand, by their key; the rest are as written.
    fold_overrides: HashMap<String, bool>,
    /// What the find bar searches for.
    query: Option<String>,
    /// The query's matches in the current text, and the text version and
    /// query they were found for.
    found: Rc<[Range<usize>]>,
    found_for: Option<(u64, String)>,
    /// Vim's state while Vim mode is on.
    vim: Option<focal_core::vim::Vim>,
    /// Sends keys to Vim before Focal's bindings see them.
    _vim_keys: gpui_kit::Subscription,
}

impl Editor {
    pub fn new(
        document: Document,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let weak = cx.entity().downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            weak.update(cx, |this, cx| this.may_close(window, cx))
                .unwrap_or(true)
        });
        cx.observe_window_appearance(window, |_, _, cx| cx.notify())
            .detach();
        let settings = cx.try_global::<Settings>().cloned().unwrap_or_default();
        cx.observe_global::<Settings>(Self::settings_changed)
            .detach();
        let fold_overrides = remembered_folds(&document, cx);
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
            reveal_frames: 0,
            table_scrolls: RefCell::default(),
            reveal_cell: Cell::new(false),
            painted: Rc::default(),
            focus_mode: false,
            focus_unit: settings.focus_unit,
            typography: Typography::new(&settings),
            typewriter: settings.typewriter,
            center_frames: 0,
            padded_for: px(0.),
            remeasured: false,
            word_count: Cell::new(None),
            conflict: false,
            error: None,
            trace: std::env::var_os("FOCAL_TRACE").is_some(),
            spell: RefCell::new(SpellChecker::new()),
            a11y_ids: RunIds::default(),
            spell_cache: RefCell::default(),
            grammar_cache: RefCell::default(),
            check_grammar: settings.check_grammar,
            correct_spelling: settings.correct_spelling && crate::spell::system_corrects_spelling(),
            highlights: RefCell::default(),
            save_task: None,
            watch: None,
            grid: None,
            next_grid_session: 0,
            last_pictures: RefCell::default(),
            math: RefCell::default(),
            diagrams: RefCell::default(),
            fetching_images: RefCell::default(),
            failed_images: RefCell::default(),
            found_files: RefCell::default(),
            pointer: Point::default(),
            frame: Rc::default(),
            footnote_preview: None,
            link_root: None,
            link_files: std::sync::Arc::from([]),
            printing: None,
            writing_tools: Self::listen_to_writing_tools(window, cx),
            table_source: None,
            pixel_scale: 1.,
            draft: None,
            fold_overrides,
            query: None,
            found: Rc::default(),
            found_for: None,
            vim: settings.vim_mode.then(focal_core::vim::Vim::new),
            _vim_keys: Self::intercept_vim_keys(cx),
        };
        // A single file's wiki links resolve among the files beside it.
        if let Some(dir) = editor
            .path()
            .and_then(std::path::Path::parent)
            .map(PathBuf::from)
        {
            let files: Vec<PathBuf> = std::fs::read_dir(&dir)
                .into_iter()
                .flatten()
                .flatten()
                .map(|entry| PathBuf::from(entry.file_name()))
                .filter(|name| {
                    name.extension()
                        .is_some_and(|e| e == "md" || e == "markdown")
                })
                .collect();
            editor.link_root = Some(dir);
            editor.link_files = files.into();
        }
        editor.refresh();
        // A document opens at its body, past any front matter.
        let body = editor.body_start();
        if body > 0 {
            editor.selection = body..body;
            editor.refresh();
        }
        editor.watch_document(cx);
        editor
    }

    pub fn title(&self) -> String {
        self.document.title()
    }

    /// The folder wiki links resolve in and its Markdown files, relative to it.
    pub(crate) fn set_link_files(
        &mut self,
        root: PathBuf,
        files: std::sync::Arc<[PathBuf]>,
        cx: &mut Context<Self>,
    ) {
        self.link_root = Some(root);
        self.link_files = files;
        cx.notify();
    }

    fn wiki_file(&self, destination: &str) -> Option<PathBuf> {
        self.sources().wiki_file(destination)
    }

    /// Where this document's links and images lead from, for export.
    fn sources(&self) -> crate::export::Sources {
        crate::export::Sources {
            document: self.document.path.clone(),
            link_root: self.link_root.clone(),
            link_files: self.link_files.clone(),
        }
    }

    /// The folder an export is offered in, the file's name without its
    /// extension, and the title: the first heading, or else the name.
    fn export_names(&self) -> (PathBuf, String, String) {
        let path = self.path();
        let folder = path.and_then(std::path::Path::parent).map_or_else(
            || std::env::current_dir().unwrap_or_default(),
            PathBuf::from,
        );
        let stem = path.and_then(std::path::Path::file_stem).map_or_else(
            || "Untitled".to_owned(),
            |s| s.to_string_lossy().into_owned(),
        );
        let title = focal_core::outline::outline(self.text())
            .into_iter()
            .next()
            .map_or_else(|| stem.clone(), |heading| heading.title);
        (folder, stem, title)
    }

    /// Asks where, then writes the document as a standalone HTML page.
    fn export_html(&mut self, _: &ExportHtml, _: &mut Window, cx: &mut Context<Self>) {
        let (folder, stem, title) = self.export_names();
        let chosen = cx.prompt_for_new_path(&folder, Some(&format!("{stem}.html")));
        let text = self.text().to_owned();
        let sources = self.sources();
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(path))) = chosen.await else {
                return;
            };
            let written = cx
                .background_executor()
                .spawn(async move { crate::export::write_page(&text, &title, &sources, &path) })
                .await;
            if let Err(error) = written {
                this.update(cx, |this, cx| {
                    this.show_error(format!("Could not export: {error:#}"), cx);
                })
                .ok();
            }
        })
        .detach();
    }

    /// Asks where, then writes the document as a PDF, set like a print.
    fn export_pdf(&mut self, _: &ExportPdf, _: &mut Window, cx: &mut Context<Self>) {
        let (folder, stem, _) = self.export_names();
        let chosen = cx.prompt_for_new_path(&folder, Some(&format!("{stem}.pdf")));
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(path))) = chosen.await else {
                return;
            };
            this.update(cx, |this, cx| {
                this.print_to(crate::print::Output::Pdf(path), cx);
            })
            .ok();
        })
        .detach();
    }

    fn print(&mut self, _: &Print, _: &mut Window, cx: &mut Context<Self>) {
        self.print_to(crate::print::Output::Printer, cx);
    }

    /// Lays the document out for paper in the background, then prints it
    /// once WebKit has loaded it, images included.
    fn print_to(&mut self, output: crate::print::Output, cx: &mut Context<Self>) {
        let (_, _, title) = self.export_names();
        let text = self.text().to_owned();
        let sources = self.sources();
        let folder = std::env::temp_dir().join(format!("focal-print-{}", std::process::id()));
        cx.spawn(async move |this, cx| {
            let printed: anyhow::Result<()> = async {
                let page =
                    cx.background_executor()
                        .spawn({
                            let title = title.clone();
                            async move {
                                crate::export::write_print_page(&text, &title, &sources, &folder)
                            }
                        })
                        .await?;
                let job = crate::print::Job::load(&page, output, &title)?;
                let id = job.id;
                this.update(cx, |this, _| this.printing = Some(job))?;
                let started = Instant::now();
                while this.read_with(cx, |this, _| {
                    this.print_job(id)
                        .is_some_and(crate::print::Job::is_loading)
                })? && started.elapsed() < PRINT_LOAD_LIMIT
                {
                    cx.background_executor()
                        .timer(Duration::from_millis(50))
                        .await;
                }
                this.update(cx, |this, _| {
                    this.printing
                        .as_mut()
                        .filter(|job| job.id == id)
                        .map_or(Ok(()), crate::print::Job::print)
                })??;
                // The web view is let go once the print panel has closed and
                // WebKit has had time to finish writing.
                while this.read_with(cx, |this, _| {
                    this.print_job(id)
                        .is_some_and(crate::print::Job::sheet_open)
                })? {
                    cx.background_executor()
                        .timer(Duration::from_millis(500))
                        .await;
                }
                cx.background_executor().timer(PRINT_GRACE).await;
                this.update(cx, |this, _| {
                    if this.print_job(id).is_some() {
                        this.printing = None;
                    }
                })?;
                Ok(())
            }
            .await;
            if let Err(error) = printed {
                this.update(cx, |this, cx| {
                    this.show_error(format!("Could not print: {error:#}"), cx);
                })
                .ok();
            }
        })
        .detach();
    }

    /// Takes text back from Writing Tools' Replace, and stops offering text
    /// when the editor goes away.
    fn listen_to_writing_tools(
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> async_channel::Sender<crate::writing_tools::Replacement> {
        let (sender, receiver) = async_channel::unbounded();
        cx.spawn(async move |this, cx| {
            while let Ok(replacement) = receiver.recv().await {
                if this
                    .update(cx, |this, cx| this.apply_writing_tools(&replacement, cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        // Another window's editor may offer its text while this one is in
        // the background.
        cx.observe_window_activation(window, |_, _, cx| cx.notify())
            .detach();
        let owner = cx.entity_id();
        cx.on_release(move |_, _| crate::writing_tools::withdraw(owner))
            .detach();
        sender
    }

    /// Offers the selection, or the whole document when nothing is
    /// selected, to Writing Tools while this editor has the focus.
    fn offer_to_writing_tools(&self, window: &Window, cx: &Context<Self>) {
        crate::writing_tools::install();
        let owner = cx.entity_id();
        if !(self.focus_handle.is_focused(window) && window.is_window_active()) {
            crate::writing_tools::withdraw(owner);
            return;
        }
        let range = if self.selection.is_empty() {
            0..self.text().len()
        } else {
            self.selection.clone()
        };
        let text = &self.text()[range.clone()];
        if crate::writing_tools::offers(owner, &range, text) {
            return;
        }
        crate::writing_tools::offer(crate::writing_tools::Target {
            owner,
            range,
            text: text.to_owned(),
            sender: self.writing_tools.clone(),
        });
    }

    /// Puts Writing Tools' new text in place of what it was given, as one
    /// undo step, selected, unless the text has changed since.
    fn apply_writing_tools(
        &mut self,
        replacement: &crate::writing_tools::Replacement,
        cx: &mut Context<Self>,
    ) {
        if self.text().get(replacement.range.clone()) != Some(replacement.original.as_str()) {
            return;
        }
        let start = replacement.range.start;
        self.edit(
            replacement.range.clone(),
            &replacement.text,
            start..start + replacement.text.len(),
            EditKind::Other,
            cx,
        );
    }

    /// The print job `id`, unless a later print replaced it.
    fn print_job(&self, id: u64) -> Option<&crate::print::Job> {
        self.printing.as_ref().filter(|job| job.id == id)
    }

    /// Copies the selection, or the whole document, as HTML and as text.
    fn copy_html(&mut self, _: &CopyHtml, _: &mut Window, cx: &mut Context<Self>) {
        let text = if self.selection.is_empty() {
            self.text().to_owned()
        } else {
            self.text()[self.selection.clone()].to_owned()
        };
        let sources = self.sources();
        let plain = text.clone();
        let html = cx
            .background_executor()
            .spawn(async move { crate::export::pasteboard_html(&text, &sources) });
        cx.spawn(async move |_, cx| {
            let html = html.await;
            cx.update(|_| crate::mac::copy_html(&html, &plain));
        })
        .detach();
    }

    /// Draws wiki links that lead nowhere yet in the marker color with a
    /// quiet underline.
    fn mark_unresolved(
        &self,
        line: usize,
        view: &LineView,
        runs: Vec<TextRun>,
        theme: &Theme,
    ) -> Vec<TextRun> {
        let analysis = &self.snapshot.analysis;
        let range = analysis.lines.range(line);
        let mut runs = runs;
        for link in &analysis.links {
            if !link.wiki || link.range.end <= range.start || range.end <= link.range.start {
                continue;
            }
            if self.link_root.is_none() || self.wiki_file(&link.destination).is_some() {
                continue;
            }
            let clamp = |offset: usize| view.map.to_display(offset.clamp(range.start, range.end));
            let marker = theme.marker;
            runs = restyle(
                runs,
                &(clamp(link.range.start)..clamp(link.range.end)),
                true,
                |part| {
                    part.color = marker;
                    part.underline = Some(UnderlineStyle {
                        thickness: px(1.),
                        color: Some(marker.opacity(0.5)),
                        wavy: false,
                    });
                },
            );
        }
        runs
    }

    /// Follows the link at `offset`: web links open in the browser, files and
    /// wiki links in Focal (a missing wiki link target as a new file beside
    /// this one), anchors move the caret. Returns whether there was a link.
    fn follow_link(&mut self, offset: usize, cx: &mut Context<Self>) -> bool {
        if self.follow_footnote(offset, cx) {
            return true;
        }
        let Some(link) = self.snapshot.analysis.link_at(offset).cloned() else {
            return false;
        };
        let destination = link.destination;
        let folder = self
            .path()
            .and_then(std::path::Path::parent)
            .map(PathBuf::from);
        if link.wiki {
            let target = self.wiki_file(&destination).or_else(|| {
                let (name, _) = links::wiki_target(&destination);
                let dir = folder.clone().or_else(|| self.link_root.clone())?;
                (!name.is_empty()).then(|| dir.join(format!("{name}.md")))
            });
            if let Some(target) = target {
                cx.emit(EditorEvent::Open(target));
            }
        } else if let Some(slug) = destination.strip_prefix('#') {
            let analysis = self.snapshot.analysis.clone();
            if let Some(at) = links::find_heading(&analysis, self.text(), slug) {
                self.jump_to(at, cx);
            }
        } else if destination.contains("://") || destination.starts_with("mailto:") {
            cx.open_url(&destination);
        } else if let Some(target) =
            folder.and_then(|dir| links::relative_target(&destination, &dir))
        {
            let markdown = target.extension().is_some_and(|e| {
                ["md", "markdown", "mdown", "mkd"]
                    .iter()
                    .any(|x| e.eq_ignore_ascii_case(x))
            });
            if markdown {
                cx.emit(EditorEvent::Open(target));
            } else {
                cx.open_url(&format!("file://{}", target.display()));
            }
        }
        true
    }

    /// A footnote reference leads to its note, a note's label back to the
    /// first reference. Returns whether `offset` was on either.
    /// Moves the caret to `offset`, which "back" returns from.
    pub(crate) fn jump_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        let from = self.selection.clone();
        self.move_to(offset, cx);
        cx.emit(EditorEvent::Jumped(from));
    }

    fn follow_footnote(&mut self, offset: usize, cx: &mut Context<Self>) -> bool {
        let analysis = &self.snapshot.analysis;
        let Some(note) = analysis.footnote_at(offset) else {
            return false;
        };
        let target = if note.definition {
            analysis
                .footnote_reference(&note.label)
                .map(|reference| reference.range.start)
        } else {
            analysis
                .footnote_definition(&note.label)
                .map(|definition| definition.body.start)
        };
        if let Some(at) = target {
            let from = self.selection.clone();
            self.move_to(at, cx);
            cx.emit(EditorEvent::Jumped(from));
        }
        true
    }

    /// The text of the note a footnote reference at `offset` points to, with
    /// Markdown markers hidden.
    fn footnote_text(&self, offset: usize) -> Option<SharedString> {
        let analysis = &self.snapshot.analysis;
        let note = analysis
            .footnote_at(offset)
            .filter(|note| !note.definition)?;
        let body = analysis.footnote_definition(&note.label)?.body.clone();
        let text: Vec<String> = analysis
            .lines
            .lines_of(&body)
            .map(|line| {
                let range = analysis.lines.range(line);
                let part = body.start.max(range.start)..body.end.min(range.end);
                range_view(analysis, self.text(), line, part, None)
                    .text
                    .trim()
                    .to_owned()
            })
            .collect();
        Some(text.join(" ").into())
    }

    /// Shows the note of the footnote reference at `offset` beside the
    /// pointer, or hides the preview.
    pub(crate) fn preview_footnote_at(&mut self, offset: Option<usize>, cx: &mut Context<Self>) {
        let preview = offset.and_then(|offset| {
            let start = self.snapshot.analysis.footnote_at(offset)?.range.start;
            Some((start, self.footnote_text(offset)?))
        });
        let unchanged = self.footnote_preview.as_ref().map(|(start, _)| *start)
            == preview.as_ref().map(|(start, _)| *start);
        if !unchanged {
            self.footnote_preview = preview;
            cx.notify();
        }
    }

    fn render_footnote_preview(&self, theme: &Theme) -> Option<impl IntoElement> {
        let (_, text) = self.footnote_preview.as_ref()?;
        // Beside the pointer, kept inside the editor.
        let frame = self.frame.get();
        let width = px(380.);
        let at = self.pointer - frame.origin;
        let left = at.x.min(frame.size.width - width - px(16.)).max(px(16.));
        Some(
            div()
                .id("footnote-preview")
                .test_support()
                .aria_label(text.clone())
                .absolute()
                .left(left)
                .top(at.y + px(20.))
                .max_w(width)
                .px(px(12.))
                .py(px(8.))
                .bg(theme.background)
                .border_1()
                .border_color(theme.rule)
                .rounded(px(6.))
                .shadow_md()
                .text_size(px(self.typography.size * 0.85))
                .line_height(relative(1.45))
                .text_color(theme.text)
                .child(text.clone()),
        )
    }

    fn open_link(&mut self, _: &OpenLink, _: &mut Window, cx: &mut Context<Self>) {
        let head = self.head();
        self.follow_link(head, cx);
    }

    /// Selects `range`, clamped to the text, as "back" does.
    pub(crate) fn select(&mut self, range: Range<usize>, cx: &mut Context<Self>) {
        let len = self.text().len();
        self.selection = range.start.min(len)..range.end.min(len);
        self.reversed = false;
        self.after_selection(cx);
    }

    pub(crate) fn show_error(&mut self, message: String, cx: &mut Context<Self>) {
        self.error = Some(message);
        cx.notify();
    }

    pub(crate) fn path(&self) -> Option<&std::path::Path> {
        self.document.path.as_deref()
    }

    /// Writes an open table cell into the text and saves, before the editor is
    /// replaced by another file's.
    pub(crate) fn flush(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_cell(window, cx);
        self.save_now(cx);
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
    /// What focus mode keeps bright around the caret, if it is on.
    fn focus(&self, analysis: &Analysis, text: &str) -> Option<Focus> {
        self.focus_mode.then(|| {
            let lines = paragraph_around(analysis, text, self.head());
            let prose = lines.clone().all(|line| {
                matches!(
                    analysis.info(line).kind,
                    LineKind::Text | LineKind::Heading(_)
                )
            });
            let sentence = (self.focus_unit == FocusUnit::Sentence && prose).then(|| {
                // In a list, a sentence never runs past its item.
                let line = analysis.lines.line_of(self.head());
                let range = if analysis.info(line).prefix.marker.is_some() {
                    analysis.content_range(line)
                } else {
                    analysis.lines.range(lines.start).start..analysis.lines.range(lines.end - 1).end
                };
                sentence_at(text, range, self.head())
            });
            Focus { lines, sentence }
        })
    }

    /// The rows of the list: lines, tables and islands, and each line's row.
    fn rows(
        &self,
        analysis: &Analysis,
        text: &str,
        line_count: usize,
        head: usize,
    ) -> (Vec<Row>, Vec<usize>, HashMap<usize, FoldTitle>) {
        let islands = crate::islands::islands(analysis, text);
        let head_line = analysis.lines.line_of(head);
        let folds: HashMap<usize, FoldTitle> = focal_core::blocks::folds(analysis, text)
            .into_iter()
            .map(|fold| {
                let folded = self
                    .fold_overrides
                    .get(&fold.key)
                    .copied()
                    .unwrap_or(fold.folded);
                let title = FoldTitle {
                    folded,
                    body: fold.body,
                    key: fold.key,
                };
                (fold.title, title)
            })
            .collect();
        let mut rows = Vec::with_capacity(line_count);
        let mut line_rows = Vec::with_capacity(line_count);
        let mut line = 0;
        while line < line_count {
            // A folded body is hidden unless the caret is in it.
            if let Some((&title, fold)) = folds.iter().find(|(_, fold)| {
                fold.folded && fold.body.start == line && !fold.body.contains(&head_line)
            }) {
                let title_row = line_rows.get(title).copied().unwrap_or(rows.len());
                for _ in fold.body.clone() {
                    line_rows.push(title_row);
                }
                line = fold.body.end;
                continue;
            }
            if let Some(island) = islands.iter().find(|island| island.start == line)
                && !(island.start..island.end).contains(&head_line)
            {
                for _ in island.start..island.end {
                    line_rows.push(rows.len());
                }
                rows.push(Row::Island(*island));
                line = island.end;
            } else if let LineKind::Table(table) = analysis.info(line).kind
                && self.table_source != Some(table)
            {
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
        (rows, line_rows, folds)
    }

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

        let (rows, line_rows, folds) = self.rows(&analysis, text, views.len(), head);

        let focus = self.focus(&analysis, text);
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
                        focus
                            .as_ref()
                            .map(|f| f.lines.contains(&line).then_some(&f.sentence))
                            .hash(&mut hasher);
                        folds.get(&line).hash(&mut hasher);
                    }
                    Row::Table(table) => {
                        text[analysis.tables[table].range.clone()].hash(&mut hasher);
                    }
                    Row::Island(island) => {
                        let start = analysis.lines.range(island.start).start;
                        let end = analysis.lines.range(island.end - 1).end;
                        text[start..end].hash(&mut hasher);
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
        self.find_matches(version);
        let text = self.buffer.text();
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
            folds: Rc::new(folds),
            text: text.into(),
            table_cells,
            a11y_document,
            line_rows,
            keys,
            focus,
        };
    }

    /// The table, row and column of the cell being edited.
    #[cfg(test)]
    pub(crate) fn editing_cell(&self) -> Option<(usize, usize, usize)> {
        self.grid.as_ref().map(|g| (g.table, g.row, g.column))
    }

    /// The lines of the caret's paragraph in focus mode.
    pub(crate) fn focus_lines(&self) -> Option<Range<usize>> {
        self.snapshot
            .focus
            .as_ref()
            .map(|focus| focus.lines.clone())
    }

    /// What focus mode keeps bright, as source bytes.
    #[cfg(test)]
    pub(crate) fn focus_range(&self) -> Option<Range<usize>> {
        let focus = self.snapshot.focus.as_ref()?;
        let lines = &self.snapshot.analysis.lines;
        Some(focus.sentence.clone().unwrap_or_else(|| {
            lines.range(focus.lines.start).start..lines.range(focus.lines.end - 1).end
        }))
    }

    #[cfg(test)]
    pub(crate) fn viewport(&self) -> Bounds<Pixels> {
        self.list.viewport_bounds()
    }

    #[cfg(test)]
    pub(crate) fn scroll_top(&self) -> (usize, Pixels) {
        let top = self.list.logical_scroll_top();
        (top.item_ix, top.offset_in_item)
    }

    /// Where the caret's row was laid out in the window, if it was.
    #[cfg(test)]
    pub(crate) fn head_row_bounds(&self) -> Option<gpui_kit::Bounds<gpui_kit::Pixels>> {
        self.list.bounds_for_item(self.head_row())
    }

    fn reveal_caret(&mut self) {
        self.reveal_frames = 4;
        self.center_frames = 4;
        let row = self.head_row();
        if self.row_in_view(row) != Some(true) {
            self.list.scroll_to_reveal_item(row);
        }
    }

    /// Whether row `row`'s text was in view in the last frame, leaving out
    /// the room below the last row; `None` when it was not laid out.
    fn row_in_view(&self, row: usize) -> Option<bool> {
        let bounds = self.list.bounds_for_item(row)?;
        let viewport = self.list.viewport_bounds();
        let last = row + 1 == self.snapshot.rows.len();
        let bottom = if last && !self.typewriter_active() {
            bounds.bottom() - END_PADDING
        } else {
            bounds.bottom()
        };
        Some(bounds.top() >= viewport.top() && bottom <= viewport.bottom())
    }

    /// Reveals the caret again once the rows the last reveal scrolled to
    /// have been measured, until its row is in view.
    fn keep_revealing(&mut self, window: &mut Window) {
        if self.reveal_frames == 0 {
            return;
        }
        self.reveal_frames -= 1;
        let row = self.head_row();
        match self.row_in_view(row) {
            Some(true) => {
                self.reveal_frames = 0;
                return;
            }
            Some(false) => self.list.scroll_to_reveal_item(row),
            // In view in the last frame, but changed since (its syntax was
            // revealed or hidden): it is laid out again in this frame, so
            // look again in the next one rather than scrolling it away.
            None if self.was_painted(row) => {}
            // Not measured yet: scroll it to the top, where it gets laid out.
            None => self.list.scroll_to(gpui_kit::ListOffset {
                item_ix: row,
                offset_in_item: px(0.),
            }),
        }
        window.request_animation_frame();
    }

    /// Whether row `row` was drawn in the last frame.
    fn was_painted(&self, row: usize) -> bool {
        self.snapshot
            .rows
            .get(row)
            .is_some_and(|row| self.painted.borrow().iter().any(|p| p.row == *row))
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

    pub(crate) fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
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
        // A table shown as source turns back into a grid once the caret leaves it.
        if let Some(table) = self.table_source
            && self.snapshot.analysis.table_at(self.head()) != Some(table)
        {
            self.table_source = None;
        }
        self.refresh();
        self.reveal_caret();
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
            self.reveal_caret();
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
        if let Some(vim) = &mut self.vim {
            vim.backspaced();
        }
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

    fn up(&mut self, _: &Up, window: &mut Window, cx: &mut Context<Self>) {
        match self.vertical(-1) {
            Vertical::To(to) => self.move_to(to, cx),
            Vertical::Table(table) => {
                let last = self.snapshot.analysis.tables[table]
                    .rows
                    .len()
                    .saturating_sub(1);
                self.edit_cell(table, last, 0, window, cx);
            }
        }
    }

    fn down(&mut self, _: &Down, window: &mut Window, cx: &mut Context<Self>) {
        match self.vertical(1) {
            Vertical::To(to) => self.move_to(to, cx),
            Vertical::Table(table) => self.edit_cell(table, 0, 0, window, cx),
        }
    }

    /// A selection may span a table: it extends to the table's start or end.
    fn select_vertically(&mut self, direction: i32, cx: &mut Context<Self>) {
        let to = match self.vertical(direction) {
            Vertical::To(to) => to,
            Vertical::Table(table) => {
                let range = &self.snapshot.analysis.tables[table].range;
                if direction < 0 {
                    range.start
                } else {
                    range.end
                }
            }
        };
        self.select_to(to, cx);
    }

    fn select_up(&mut self, _: &SelectUp, _: &mut Window, cx: &mut Context<Self>) {
        self.select_vertically(-1, cx);
    }

    fn select_down(&mut self, _: &SelectDown, _: &mut Window, cx: &mut Context<Self>) {
        self.select_vertically(1, cx);
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
        if let Some(vim) = &mut self.vim {
            vim.typed("\n");
        }
        self.insert_newline(cx);
    }

    /// A new line that continues the list or quote it is typed in.
    fn insert_newline(&mut self, cx: &mut Context<Self>) {
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

    fn undo(&mut self, _: &Undo, window: &mut Window, cx: &mut Context<Self>) {
        self.close_cell(window, cx);
        if let Some(selection) = self.buffer.undo() {
            self.restore(selection, cx);
        }
    }

    fn redo(&mut self, _: &Redo, window: &mut Window, cx: &mut Context<Self>) {
        self.close_cell(window, cx);
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
        self.reveal_caret();
        self.schedule_save(cx);
        cx.notify();
    }

    fn bold(&mut self, _: &Bold, _: &mut Window, cx: &mut Context<Self>) {
        let change = editing::toggle_wrap(self.text(), &self.selection, "**");
        self.apply(change, cx);
    }

    fn highlight(&mut self, _: &Highlight, _: &mut Window, cx: &mut Context<Self>) {
        let change = editing::toggle_wrap(self.text(), &self.selection, "==");
        self.apply(change, cx);
    }

    /// Shows the file in a Finder window, selected.
    fn show_in_finder(&mut self, _: &ShowInFinder, _: &mut Window, _: &mut Context<Self>) {
        if let Some(path) = self.path()
            && let Err(error) = std::process::Command::new("open")
                .arg("-R")
                .arg(path)
                .spawn()
        {
            eprintln!(
                "focal: could not show {} in Finder: {error}",
                path.display()
            );
        }
    }

    fn italic(&mut self, _: &Italic, _: &mut Window, cx: &mut Context<Self>) {
        let change = editing::toggle_wrap(self.text(), &self.selection, "_");
        self.apply(change, cx);
    }

    fn strikethrough(&mut self, _: &Strikethrough, _: &mut Window, cx: &mut Context<Self>) {
        let change = editing::toggle_wrap(self.text(), &self.selection, "~~");
        self.apply(change, cx);
    }

    fn inline_code(&mut self, _: &InlineCode, _: &mut Window, cx: &mut Context<Self>) {
        let change = editing::toggle_wrap(self.text(), &self.selection, "`");
        self.apply(change, cx);
    }

    fn insert_link(&mut self, _: &InsertLink, _: &mut Window, cx: &mut Context<Self>) {
        let change = editing::insert_link(self.text(), &self.selection);
        self.apply(change, cx);
    }

    fn set_heading(&mut self, action: &SetHeading, _: &mut Window, cx: &mut Context<Self>) {
        let lines = &self.snapshot.analysis.lines;
        let change = editing::set_heading(self.text(), lines, &self.selection, action.0);
        self.apply(change, cx);
    }

    fn toggle_prefix(&mut self, prefix: editing::BlockPrefix, cx: &mut Context<Self>) {
        let lines = &self.snapshot.analysis.lines;
        let change = editing::toggle_prefix(self.text(), lines, &self.selection, prefix);
        self.apply(change, cx);
    }

    fn toggle_bullets(&mut self, _: &ToggleBullets, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_prefix(editing::BlockPrefix::Bullet, cx);
    }

    fn toggle_numbers(&mut self, _: &ToggleNumbers, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_prefix(editing::BlockPrefix::Numbered, cx);
    }

    fn toggle_task(&mut self, _: &ToggleTask, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_prefix(editing::BlockPrefix::Task, cx);
    }

    fn toggle_quote(&mut self, _: &ToggleQuote, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_prefix(editing::BlockPrefix::Quote, cx);
    }

    fn insert_block(&mut self, block: editing::Block, cx: &mut Context<Self>) {
        let lines = &self.snapshot.analysis.lines;
        let change = editing::insert_block(self.text(), lines, &self.selection, block);
        self.apply(change, cx);
    }

    /// Inserts a table and starts editing its first cell.
    fn insert_table(&mut self, _: &InsertTable, window: &mut Window, cx: &mut Context<Self>) {
        self.insert_block(editing::Block::Table, cx);
        if let Some(table) = self.snapshot.analysis.table_at(self.head()) {
            self.edit_cell(table, 0, 0, window, cx);
        }
    }

    fn insert_code_block(&mut self, _: &InsertCodeBlock, _: &mut Window, cx: &mut Context<Self>) {
        self.insert_block(editing::Block::CodeBlock, cx);
    }

    fn insert_math(&mut self, _: &InsertMath, _: &mut Window, cx: &mut Context<Self>) {
        self.insert_block(editing::Block::Math, cx);
    }

    pub(crate) const fn focus_mode(&self) -> bool {
        self.focus_mode
    }

    pub(crate) fn bar_state(&self) -> BarState {
        let analysis = &self.snapshot.analysis;
        let line = analysis.lines.line_of(self.head());
        let heading = match analysis.info(line).kind {
            LineKind::Heading(level) => level,
            _ => 0,
        };
        let version = self.snapshot.version;
        let words = match self.word_count.get() {
            Some((cached, words)) if Some(cached) == version => words,
            _ => {
                let words = word_count(self.text());
                if let Some(version) = version {
                    self.word_count.set(Some((version, words)));
                }
                words
            }
        };
        // A file that is not plain UTF-8 says how it is stored.
        let format = &self.document.format;
        let stored = [format.name(), format.cr_only.then_some("CR line endings")];
        let mut file_name = self.title();
        for note in stored.into_iter().flatten() {
            file_name.push_str(" · ");
            file_name.push_str(note);
        }
        BarState {
            file_name: file_name.into(),
            heading,
            words,
            selected_words: word_count(&self.text()[self.selection.clone()]),
            minutes: reading_minutes(words),
        }
    }

    fn toggle_focus_mode(&mut self, _: &ToggleFocusMode, _: &mut Window, cx: &mut Context<Self>) {
        self.focus_mode = !self.focus_mode;
        self.refresh();
        // The first and last rows' padding depends on typewriter scrolling.
        self.remeasure();
        self.center_frames = 4;
        cx.notify();
    }

    fn remeasure(&mut self) {
        self.list.remeasure();
        self.remeasured = true;
    }

    const fn typewriter_active(&self) -> bool {
        self.focus_mode && self.typewriter
    }

    /// Scrolls the caret's line to the vertical center, using where it was
    /// drawn in the last frame, until it is there.
    fn keep_centering(&mut self, window: &mut Window) {
        if !self.typewriter_active() || self.grid.is_some() || self.center_frames == 0 {
            return;
        }
        // Rows are unmeasured right after a remeasure, and `scroll_by` walks
        // their heights; wait for a frame to lay them out.
        if self.reveal_frames > 0 || std::mem::take(&mut self.remeasured) {
            window.request_animation_frame();
            return;
        }
        self.center_frames -= 1;
        let head = self.head();
        let line = self.snapshot.analysis.lines.line_of(head);
        let caret = self.painted.borrow().iter().find_map(|painted| {
            if painted.row != Row::Line(line) {
                return None;
            }
            let (layout, view) = (painted.layout.as_ref()?, painted.view.as_ref()?);
            let position = layout.position_for_index(view.map.to_display(head))?;
            Some(position.y + layout.line_height() / 2.)
        });
        let Some(caret) = caret else {
            window.request_animation_frame();
            return;
        };
        let delta = caret - self.list.viewport_bounds().center().y;
        let scrolled = -self.list.scroll_px_offset_for_scrollbar().y;
        if delta.abs() <= px(0.5) || (delta < px(0.) && scrolled <= px(0.)) {
            self.center_frames = 0;
            return;
        }
        if scrolled + delta <= px(0.) {
            self.list.scroll_to(gpui_kit::ListOffset {
                item_ix: 0,
                offset_in_item: px(0.),
            });
        } else {
            self.list.scroll_by(delta);
        }
        window.request_animation_frame();
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
        self.save_as(false, window, cx);
    }

    /// Saves, asking where for an untitled document, then closes the window
    /// when `close`.
    fn save_as(&mut self, close: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.document.path.is_some() {
            self.conflict = false;
            self.save_now(cx);
            if close {
                window.remove_window();
            }
            return;
        }
        let directory = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let chosen = cx.prompt_for_new_path(&directory, Some("Untitled.md"));
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(path))) = chosen.await {
                this.update_in(cx, |this, window, cx| {
                    this.document.path = Some(path.clone());
                    this.watch_document(cx);
                    this.save_now(cx);
                    this.discard_draft();
                    window.set_window_title(&this.title());
                    crate::recent::note(&path, cx);
                    if close {
                        window.remove_window();
                    }
                })
                .ok();
            }
        })
        .detach();
    }

    /// Why closing would lose text, if it would: an untitled document with
    /// unsaved edits that leave some text, or edits made while the file
    /// changed on disk (which are not saved over it without asking).
    fn close_question(&self) -> Option<CloseQuestion> {
        if !self.dirty() {
            None
        } else if self.conflict {
            Some(CloseQuestion::Conflict)
        } else if self.document.path.is_none() && !self.text().is_empty() {
            Some(CloseQuestion::Untitled)
        } else {
            None
        }
    }

    pub(crate) fn asks_before_closing(&self) -> bool {
        self.close_question().is_some()
    }

    /// Saves, then tells whether the window may close now. Text that would
    /// be lost (see [`Self::close_question`]) asks first and closes the window
    /// itself once answered.
    fn may_close(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(question) = self.close_question() else {
            self.save_now(cx);
            self.discard_draft();
            return true;
        };
        let title = self.title();
        cx.spawn_in(window, async move |this, cx| {
            match crate::mac::ask_on_close(question, &title) {
                SaveAnswer::Save => this
                    .update_in(cx, |this, window, cx| {
                        if question == CloseQuestion::Conflict {
                            this.keep_mine(cx);
                            window.remove_window();
                        } else {
                            this.save_as(true, window, cx);
                        }
                    })
                    .ok(),
                SaveAnswer::DontSave => this
                    .update_in(cx, |this, window, _| {
                        this.discard_draft();
                        window.remove_window();
                    })
                    .ok(),
                SaveAnswer::Cancel => None,
            };
        })
        .detach();
        false
    }

    fn close_window(&mut self, _: &CloseWindow, window: &mut Window, cx: &mut Context<Self>) {
        if self.may_close(window, cx) {
            window.remove_window();
        }
    }

    /// Saves what can be saved, as the app quits.
    pub(crate) fn save_before_quit(&mut self, cx: &mut Context<Self>) {
        self.save_now(cx);
        self.save_draft(cx);
    }

    // ---- Find --------------------------------------------------------------

    /// Finds the query's matches again when the text or the query changed.
    fn find_matches(&mut self, version: u64) {
        let Some(query) = &self.query else {
            self.found = Rc::default();
            self.found_for = None;
            return;
        };
        if self.found_for.as_ref() != Some(&(version, query.clone())) {
            self.found = find::find_all(self.buffer.text(), query).into();
            self.found_for = Some((version, query.clone()));
        }
    }

    /// Searches for `query` (or stops searching with `None`), selecting the
    /// first match at or after the selection.
    pub(crate) fn set_query(&mut self, query: Option<String>, cx: &mut Context<Self>) {
        let query = query.filter(|query| !query.is_empty());
        if query == self.query {
            return;
        }
        self.query = query;
        self.find_matches(self.buffer.version());
        match find::next_match(&self.found, self.selection.start) {
            Some(ix) => self.select(self.found[ix].clone(), cx),
            None => cx.notify(),
        }
    }

    /// Selects the next match, or the previous one, wrapping around.
    pub(crate) fn find_step(&mut self, forward: bool, cx: &mut Context<Self>) {
        let found = if forward {
            find::next_match(&self.found, self.selection.end)
        } else {
            find::previous_match(&self.found, self.selection.start)
        };
        if let Some(ix) = found {
            self.select(self.found[ix].clone(), cx);
        }
    }

    /// Selects the next or previous match of `query` without highlighting
    /// the others, as ⌘G does with the find bar closed.
    pub(crate) fn find_again(&mut self, query: String, forward: bool, cx: &mut Context<Self>) {
        self.query = Some(query);
        self.find_matches(self.buffer.version());
        self.find_step(forward, cx);
        self.query = None;
        self.find_matches(self.buffer.version());
        cx.notify();
    }

    /// The selected text, when it fits on one line, to search for.
    pub(crate) fn selected_line_text(&self) -> Option<String> {
        let text = &self.text()[self.selection.clone()];
        (!text.is_empty() && !text.contains('\n')).then(|| text.to_owned())
    }

    /// Replaces the selected match and selects the next one. With no match
    /// selected, only moves to the next match, so a stray selection is never
    /// replaced.
    pub(crate) fn replace_current(&mut self, replacement: &str, cx: &mut Context<Self>) {
        if !self.found.contains(&self.selection) {
            self.find_step(true, cx);
            return;
        }
        let range = self.selection.clone();
        let end = range.start + replacement.len();
        self.edit(range, replacement, end..end, EditKind::Other, cx);
        if let Some(ix) = find::next_match(&self.found, end) {
            self.select(self.found[ix].clone(), cx);
        }
    }

    /// Replaces every match, as one undo step.
    pub(crate) fn replace_all(&mut self, replacement: &str, cx: &mut Context<Self>) {
        if let Some(change) = find::replace_all(&self.found, self.text(), replacement) {
            self.apply(change, cx);
        }
    }

    /// While searching: which match is selected, and how many there are.
    pub(crate) fn find_status(&self) -> Option<(Option<usize>, usize)> {
        self.query.as_ref()?;
        let current = self.found.iter().position(|m| *m == self.selection);
        Some((current, self.found.len()))
    }

    // ---- Files -----------------------------------------------------------

    fn schedule_save(&mut self, cx: &mut Context<Self>) {
        if self.document.path.is_none() {
            self.save_task = Some(cx.spawn(async move |this, cx| {
                cx.background_executor().timer(AUTOSAVE_DELAY).await;
                this.update(cx, |this, cx| this.save_draft(cx)).ok();
            }));
            return;
        }
        if self.conflict {
            return;
        }
        self.save_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(AUTOSAVE_DELAY).await;
            this.update(cx, |this, cx| this.save_now(cx)).ok();
        }));
    }

    /// Keeps an untitled document's unsaved text in its draft, or removes
    /// the draft once there is nothing left to keep.
    fn save_draft(&mut self, cx: &mut Context<Self>) {
        self.save_task = None;
        if self.document.path.is_some() {
            return;
        }
        if !self.asks_before_closing() {
            self.discard_draft();
            return;
        }
        let drafts = cx
            .try_global::<crate::drafts::Drafts>()
            .cloned()
            .unwrap_or_default();
        if self.draft.is_none() {
            self.draft = drafts.new_path();
        }
        if let Some(draft) = &self.draft
            && let Err(error) = crate::drafts::write(draft, self.buffer.text())
        {
            self.error = Some(format!("Could not keep a draft: {error:#}"));
            cx.notify();
        }
    }

    /// Removes this document's draft: its text was saved or discarded.
    pub(crate) fn discard_draft(&mut self) {
        if let Some(draft) = self.draft.take() {
            crate::drafts::remove(&draft);
        }
    }

    /// Continues the draft at `path`, as unsaved text.
    pub(crate) fn restore_draft(&mut self, path: PathBuf) {
        self.draft = Some(path);
        // No file holds this text yet, whatever the buffer's version.
        self.document.saved_version = u64::MAX;
    }

    fn save_now(&mut self, cx: &mut Context<Self>) {
        self.save_task = None;
        if !self.dirty() || self.conflict {
            return;
        }
        let version = self.buffer.version();
        match self.document.save(self.buffer.text(), version) {
            // A notice (saved as UTF-8 after all) shows in the banner.
            Ok(()) => self.error = self.document.notice.take(),
            Err(error) => self.error = Some(format!("Could not save: {error:#}")),
        }
        cx.notify();
    }

    /// Watches the document's file through FSEvents and reloads it on change.
    fn watch_document(&mut self, cx: &mut Context<Self>) {
        self.watch = None;
        let Some(path) = self.document.path.clone().filter(|_| document::WATCH_FILES) else {
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
    pub(crate) fn check_disk(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.document.path.clone() else {
            return;
        };
        let stamp = Stamp::of(&path);
        if stamp.is_none() || stamp == self.document.stamp {
            return;
        }
        let Ok((text, format)) = document::read(&path) else {
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
        self.load_theirs(&text, format, stamp, cx);
    }

    fn load_theirs(
        &mut self,
        text: &str,
        format: focal_core::encoding::Format,
        stamp: Option<Stamp>,
        cx: &mut Context<Self>,
    ) {
        self.document.format = format;
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
            Ok((text, format)) => self.load_theirs(&text, format, Stamp::of(&path), cx),
            Err(error) => {
                self.error = Some(format!("Could not reload: {error:#}"));
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
            Row::Island(island) => Some(Hit::Island(island)),
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
    fn vertical(&mut self, direction: i32) -> Vertical {
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
            return Vertical::To(self.vertical_by_column(direction));
        };
        let Some(position) = caret_position(&layout, view.map.to_display(head)) else {
            return Vertical::To(self.vertical_by_column(direction));
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
                return Vertical::To(target);
            }
        }
        // The next line in a different row: lines hidden in a fold share
        // their title's row and are stepped over.
        let line_rows = &self.snapshot.line_rows;
        let current_row = line_rows.get(line).copied();
        let mut target = line;
        loop {
            target = if direction < 0 {
                match target.checked_sub(1) {
                    Some(target) => target,
                    None => return Vertical::To(0),
                }
            } else if target + 1 < analysis.line_count() {
                target + 1
            } else {
                return Vertical::To(self.text().len());
            };
            if line_rows.get(target).copied() != current_row {
                break;
            }
        }
        let target_row = line_rows
            .get(target)
            .and_then(|&row| self.snapshot.rows.get(row));
        // Onto a folded block from below: onto its title.
        if let Some(&Row::Line(shown)) = target_row {
            target = shown;
        }
        if let Some(&Row::Table(table)) = target_row {
            return Vertical::Table(table);
        }
        // Into an island: onto its last line from below, its first from above.
        if let Some(&Row::Island(island)) = target_row {
            let line = if direction < 0 {
                island.end - 1
            } else {
                island.start
            };
            return Vertical::To(self.snapshot.analysis.lines.range(line).start);
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
                Vertical::To(view.map.to_source(display_index(&layout, point(x, y))))
            }
            None => Vertical::To(self.vertical_by_column(direction)),
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
        if let Some(vim) = &mut self.vim {
            vim.reset();
        }
        let Some(hit) = self.hit_test(event.position) else {
            return;
        };
        let offset = match hit {
            Hit::Table(table) => {
                self.edit_cell(table, 0, 0, window, cx);
                return;
            }
            Hit::Island(island) => {
                let entry = self.island_entry(island);
                self.move_to(entry, cx);
                return;
            }
            Hit::Text { offset } => offset,
        };
        self.grid = None;
        if event.modifiers.platform && self.follow_link(offset, cx) {
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
            self.pointer = event.position;
            let offset = match self.hit_test(event.position) {
                Some(Hit::Text { offset }) => Some(offset),
                _ => None,
            };
            self.preview_footnote_at(offset, cx);
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
        let fade = |color: Hsla| if dimmed { color.opacity(DIMMED) } else { color };
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
        let markable = self.markable(line, view, hide_at_caret);
        found
            .iter()
            .filter(|word| markable(word))
            .cloned()
            .collect()
    }

    /// Whether a display range found by a checker is marked: it lies in
    /// prose, and, when `hide_at_caret`, the caret is not in it.
    fn markable(
        &self,
        line: usize,
        view: &LineView,
        hide_at_caret: bool,
    ) -> impl Fn(&Range<usize>) -> bool + use<> {
        let prose = prose_ranges(view);
        let head = self.head();
        let line_range = self.snapshot.analysis.content_range(line);
        let caret = (hide_at_caret && line_range.start <= head && head <= line_range.end)
            .then(|| view.map.to_display(head));
        move |range| {
            prose
                .iter()
                .any(|p| p.start <= range.start && range.end <= p.end)
                && caret.is_none_or(|at| at < range.start || at > range.end)
        }
    }

    /// The grammar issues on `line` that are underlined, in display ranges.
    pub(crate) fn grammar_issues(&self, line: usize, hide_at_caret: bool) -> Vec<GrammarIssue> {
        let view = &self.snapshot.views[line];
        let kind = &self.snapshot.analysis.info(line).kind;
        if !self.check_grammar
            || !matches!(kind, LineKind::Text | LineKind::Heading(_))
            || view.text.trim().is_empty()
        {
            return Vec::new();
        }
        let found = {
            let mut cache = self.grammar_cache.borrow_mut();
            if cache.len() > SPELL_CACHE_LIMIT {
                cache.clear();
            }
            cache
                .entry(view.text.clone())
                .or_insert_with(|| self.spell.borrow().grammar(&view.text).into())
                .clone()
        };
        let markable = self.markable(line, view, hide_at_caret);
        found
            .iter()
            .filter(|issue| markable(&issue.range))
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
            if let Some(issue) = self
                .grammar_issues(line, false)
                .into_iter()
                .find(|issue| issue.range.start <= at && at <= issue.range.end)
            {
                self.grammar_menu(&view, &issue, event.position, window, cx);
            }
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

    fn settings_changed(&mut self, cx: &mut Context<Self>) {
        let settings = cx.global::<Settings>();
        self.focus_unit = settings.focus_unit;
        self.typography = Typography::new(settings);
        self.typewriter = settings.typewriter;
        self.check_grammar = settings.check_grammar;
        self.correct_spelling =
            settings.correct_spelling && crate::spell::system_corrects_spelling();
        self.set_vim_mode(settings.vim_mode, cx);
        self.refresh();
        self.remeasure();
        cx.notify();
    }

    /// After `typed` finishes a word, replaces the word with macOS's
    /// correction when it is misspelled, as an undo step of its own. Only a
    /// word standing alone in prose is corrected: not code, math, links'
    /// addresses, paths or identifiers.
    fn correct_finished_word(&mut self, typed: &str, cx: &mut Context<Self>) {
        let mut chars = typed.chars();
        let (Some(boundary), None) = (chars.next(), chars.next()) else {
            return;
        };
        if !self.correct_spelling
            || !self.selection.is_empty()
            || !(boundary.is_whitespace() || ".,;:!?)".contains(boundary))
        {
            return;
        }
        let end = self.selection.start - typed.len();
        let line = self.snapshot.analysis.lines.line_of(end);
        let line_start = self.snapshot.analysis.content_range(line).start;
        let before = &self.text()[line_start.min(end)..end];
        let Some(start) = before
            .char_indices()
            .rev()
            .take_while(|&(_, c)| c.is_alphabetic() || c == '\'')
            .last()
            .map(|(ix, _)| line_start + ix)
        else {
            return;
        };
        let start = start
            + (self.text()[start..end].len()
                - self.text()[start..end].trim_start_matches('\'').len());
        let standalone = self.text()[..start]
            .chars()
            .next_back()
            .is_none_or(|c| start == line_start || c.is_whitespace() || "([\"*_~“‘".contains(c));
        if start >= end || !standalone {
            return;
        }
        let view = self.snapshot.views[line].clone();
        let kind = self.snapshot.analysis.info(line).kind.clone();
        let word = view.map.to_display(start)..view.map.to_display(end);
        if !matches!(kind, LineKind::Text | LineKind::Heading(_))
            || !self.markable(line, &view, false)(&word)
        {
            return;
        }
        // The whole line goes along, so the word is judged in the language
        // the line is written in.
        let Some(correction) = self.spell.borrow().correction(&view.text, word) else {
            return;
        };
        let caret = self.selection.start - (end - start) + correction.len();
        self.edit(start..end, &correction, caret..caret, EditKind::Other, cx);
    }

    /// The context menu for a grammar issue: what the checker says, and its
    /// corrections.
    fn grammar_menu(
        &mut self,
        view: &LineView,
        issue: &GrammarIssue,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let source = view.map.to_source(issue.range.start)..view.map.to_source(issue.range.end);
        let text = self.text()[source.clone()].to_owned();
        self.selection = source.clone();
        self.reversed = false;
        self.after_selection(cx);
        let replace = |replacement: &str| ReplaceWord {
            range: source.clone(),
            word: text.clone(),
            replacement: replacement.to_owned(),
        };
        let mut menu = NativeMenu::new().menu_with_disabled(
            issue.description.clone(),
            true,
            Box::new(replace(&text)),
        );
        if !issue.corrections.is_empty() {
            menu = menu.separator();
        }
        for correction in issue.corrections.iter().take(6) {
            menu = menu.menu(correction.clone(), Box::new(replace(correction)));
        }
        menu.show(position, window, cx);
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
            // With the caret on an image's line, the image shows below its source.
            Row::Line(line) => {
                let text = self.render_line(line, &theme, window, cx);
                let analysis = &self.snapshot.analysis;
                // A formula being edited shows its preview after its last line.
                let math = math_blocks(analysis, self.text())
                    .into_iter()
                    .find(|block| block.lines.end == line + 1);
                let diagram = diagram_blocks(analysis, self.text())
                    .into_iter()
                    .filter(|block| crate::diagram::can_draw(block.language))
                    .find(|block| block.lines.end == line + 1);
                if block_image(analysis, self.text(), line).is_some() {
                    div()
                        .child(text)
                        .child(self.render_image(line, "image-preview", &theme, cx))
                        .into_any_element()
                } else if let Some(block) = diagram {
                    div()
                        .child(text)
                        .child(self.render_diagram(
                            block.lines.start,
                            "diagram-preview",
                            &theme,
                            cx,
                        ))
                        .into_any_element()
                } else if let Some(block) = math {
                    div()
                        .child(text)
                        .child(self.render_math(block.lines.start, "math-preview", &theme, cx))
                        .into_any_element()
                } else {
                    text
                }
            }
            Row::Table(table) => self.render_table(table, &theme, window, cx),
            Row::Island(island) => self.render_island(island, &theme, cx),
        };
        div()
            .w_full()
            .flex()
            .justify_center()
            .px(px(48.))
            // Typewriter scrolling needs room to center the first and last lines.
            .when(ix == 0, |d| {
                d.pt(if self.typewriter_active() {
                    window.viewport_size().height / 2.
                } else {
                    px(56.)
                })
            })
            .when(last, |d| {
                d.pb(if self.typewriter_active() {
                    window.viewport_size().height / 2.
                } else {
                    END_PADDING
                })
            })
            .child(
                div()
                    .w_full()
                    .max_w(px(self.typography.column))
                    .child(content),
            )
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
        // Focus mode dims the syntax around the words, even where it is bright.
        let focused = theme.focused();
        let theme = if self.focus_mode { &focused } else { theme };
        let analysis = &self.snapshot.analysis;
        let info = analysis.info(line).clone();
        let view = self.snapshot.views[line].clone();
        let (dimmed, keep) = match &self.snapshot.focus {
            Some(focus) if !focus.lines.contains(&line) => (true, None),
            Some(focus) => (false, focus.sentence.clone()),
            None => (false, None),
        };
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
        let family = if mono {
            MONO_FONT
        } else {
            self.typography.prose
        };
        let mut base = if dimmed {
            theme.text.opacity(DIMMED)
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
        let grammar: Vec<Range<usize>> = self
            .grammar_issues(line, true)
            .into_iter()
            .map(|issue| issue.range)
            .collect();
        let marked = mark_runs(&marked, &grammar, InlineStyle::GRAMMAR);
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
        // A list marker or quote bar dims with its line, or with a sentence
        // elsewhere in the paragraph.
        let prefix_dimmed = dimmed
            || keep.as_ref().is_some_and(|keep| {
                let range = analysis.lines.range(line);
                keep.end < range.start || keep.start > range.end
            });
        // By sentence, the rest of the paragraph is dimmed too.
        let runs = match keep {
            Some(keep) => {
                let range = analysis.lines.range(line);
                let clamp =
                    |offset: usize| view.map.to_display(offset.clamp(range.start, range.end));
                fade_outside(runs, &(clamp(keep.start)..clamp(keep.end)))
            }
            None => runs,
        };
        let runs = self.mark_unresolved(line, &view, runs, theme);
        let text = StyledText::new(view.text.clone()).with_runs(runs);
        let layout = text.layout().clone();
        let line_range = analysis.content_range(line);
        let code_fence = matches!(info.kind, LineKind::CodeFence { .. });
        let code = matches!(info.kind, LineKind::Code) || code_fence;
        let rule = info.kind == LineKind::ThematicBreak && view.text.is_empty();

        let selection = self.selection.clone();
        let head = self.head();
        // Vim's block cursor, over the character it is on.
        let block = self
            .vim_block()
            .map(|at| (at, self.buffer.next_grapheme(at).min(line_range.end)));
        let focused = self.focus_handle.clone();
        let painted = self.painted.clone();
        let paint_layout = layout.clone();
        let paint_view = view.clone();
        let selection_color = theme.selection;
        let found_color = theme.found;
        let found = self.found_on(&line_range);
        let caret_color = theme.caret;
        let _ = (window, cx);

        let content = div()
            .relative()
            .pt(px(top))
            .font_family(family)
            .text_size(px(self.typography.size * scale))
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
                        .text_size(px(self.typography.size * 0.7))
                },
            )
            .when(
                matches!(info.kind, LineKind::CodeFence { opening: false }),
                |d| {
                    d.rounded_bl(px(6.))
                        .rounded_br(px(6.))
                        .text_size(px(self.typography.size * 0.7))
                },
            )
            .child(
                canvas(|_, _, _| {}, {
                    let layout = layout.clone();
                    let view = view.clone();
                    let selection = selection.clone();
                    let line_range = line_range.clone();
                    move |_, (), window, _| {
                        for found in &found {
                            paint_selection(
                                &layout,
                                &view,
                                found,
                                &line_range,
                                found_color,
                                window,
                            );
                        }
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
                        .top(px(self.typography.size * 0.8))
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
                        if !focused.is_focused(window) {
                            return;
                        }
                        let height = paint_layout.line_height();
                        if let Some((at, end)) = block {
                            if line_range.start <= at
                                && at <= line_range.end
                                && let Some(position) =
                                    caret_position(&paint_layout, paint_view.map.to_display(at))
                            {
                                // As wide as the character, or half a line on
                                // an empty line or at a wrap.
                                let width = paint_layout
                                    .position_for_index(paint_view.map.to_display(end))
                                    .filter(|next| next.y == position.y && next.x > position.x)
                                    .map_or(height * 0.5, |next| next.x - position.x);
                                window.paint_quad(fill(
                                    Bounds::new(position, size(width, height)),
                                    caret_color.opacity(0.35),
                                ));
                            }
                            return;
                        }
                        let on_line = line_range.start <= head && head <= line_range.end;
                        if on_line
                            && let Some(position) =
                                caret_position(&paint_layout, paint_view.map.to_display(head))
                        {
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
        let line_height = px(self.typography.size
            * scale
            * if matches!(info.kind, LineKind::Heading(_)) {
                1.3
            } else {
                1.6
            });
        // HTML centers README titles and badges.
        let content = if info.centered {
            div()
                .w_full()
                .flex()
                .justify_center()
                .child(content.max_w_full())
                .into_any_element()
        } else {
            content.into_any_element()
        };
        let content = match self.snapshot.folds.get(&line) {
            Some(fold) => self.with_fold_toggle(content, line, fold, theme, cx),
            None => content,
        };
        crate::prefix::wrap(
            content,
            &info.prefix,
            line,
            // Focus mode dims bullets, numbers and quote bars with the syntax.
            &if prefix_dimmed || self.focus_mode {
                theme.faded()
            } else {
                *theme
            },
            line_height,
            on_toggle,
        )
    }

    /// The find matches inside a table `cell`, as ranges of its displayed
    /// `view`, each with whether it is the selected (current) match.
    pub(crate) fn cell_highlights(
        &self,
        cell: &Range<usize>,
        view: &LineView,
    ) -> Vec<(Range<usize>, bool)> {
        self.found_on(cell)
            .into_iter()
            .filter(|m| m.start >= cell.start && m.end <= cell.end)
            .map(|m| {
                let current = m == self.selection;
                (
                    view.map.to_display(m.start)..view.map.to_display(m.end),
                    current,
                )
            })
            .collect()
    }

    /// A fold's title line with its toggle beside it: ▸ while folded, ▾ open.
    fn with_fold_toggle(
        &self,
        content: gpui_kit::AnyElement,
        line: usize,
        fold: &FoldTitle,
        theme: &Theme,
        cx: &Context<Self>,
    ) -> gpui_kit::AnyElement {
        let key = fold.key.clone();
        let hidden = fold.body.len();
        let (label, help) = if fold.folded {
            let lines = if hidden == 1 { "line" } else { "lines" };
            (format!("▸ {hidden} {lines}"), "Expand")
        } else {
            ("▾".to_owned(), "Collapse")
        };
        div()
            .flex()
            .items_center()
            .child(div().flex_1().min_w(px(0.)).child(content))
            .child(
                div()
                    .id(("fold-toggle", line))
                    .test_support()
                    .aria_label(help)
                    .flex_none()
                    .px(px(8.))
                    .cursor_pointer()
                    .text_size(px(self.typography.size * 0.75))
                    .text_color(theme.marker)
                    .child(label)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.toggle_fold(&key, cx);
                        }),
                    ),
            )
            .into_any_element()
    }

    /// Opens or closes the fold named `key`. Folding with the caret inside
    /// moves the caret to the fold's title.
    pub(crate) fn toggle_fold(&mut self, key: &str, cx: &mut Context<Self>) {
        let Some((title, fold)) = self
            .snapshot
            .folds
            .iter()
            .find(|(_, fold)| fold.key == key)
            .map(|(line, fold)| (*line, fold.clone()))
        else {
            return;
        };
        let folded = !fold.folded;
        self.fold_overrides.insert(key.to_owned(), folded);
        if let Some(path) = self.document.path.clone()
            && cx.has_global::<crate::fold_memory::FoldMemory>()
        {
            cx.global_mut::<crate::fold_memory::FoldMemory>()
                .set(&path, key, folded);
        }
        let head_line = self.snapshot.analysis.lines.line_of(self.head());
        if folded && fold.body.contains(&head_line) {
            let end = self.snapshot.analysis.lines.range(title).end;
            self.move_to(end, cx);
            return;
        }
        self.refresh();
        cx.notify();
    }

    /// The matches that touch `line`.
    pub(crate) fn found_on(&self, line: &Range<usize>) -> Vec<Range<usize>> {
        let first = self.found.partition_point(|m| m.end < line.start);
        self.found[first..]
            .iter()
            .take_while(|m| m.start <= line.end)
            .cloned()
            .collect()
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
                .id("error-banner")
                .test_support()
                .aria_label(error.clone())
                .child(error.clone())
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
    let fade = |color: Hsla| if dimmed { color.opacity(DIMMED) } else { color };
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
                || style.contains(InlineStyle::TAG)
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
                .then(|| fade(theme.highlight));
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
                } else if style.contains(InlineStyle::GRAMMAR) {
                    Some(UnderlineStyle {
                        color: Some(theme.grammar),
                        thickness: px(1.5),
                        wavy: false,
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
/// Splits text runs at the ends of `range` (display offsets of the line) and
/// changes the parts inside it, or outside it.
fn restyle(
    runs: Vec<TextRun>,
    range: &Range<usize>,
    inside: bool,
    change: impl Fn(&mut TextRun),
) -> Vec<TextRun> {
    let mut out = Vec::with_capacity(runs.len() + 2);
    let mut at = 0;
    for run in runs {
        let end = at + run.len;
        let mut cuts = vec![at];
        cuts.extend(
            [range.start, range.end]
                .into_iter()
                .filter(|&c| at < c && c < end),
        );
        cuts.push(end);
        for piece in cuts.windows(2) {
            let mut part = run.clone();
            part.len = piece[1] - piece[0];
            let within = !range.is_empty() && range.start <= piece[0] && piece[1] <= range.end;
            if within == inside {
                change(&mut part);
            }
            out.push(part);
        }
        at = end;
    }
    out
}

/// Dims the runs outside `keep`, a display range of the line.
/// The folds opened or closed by hand when `document` was last open.
fn remembered_folds(document: &Document, cx: &App) -> HashMap<String, bool> {
    document
        .path
        .as_deref()
        .and_then(|path| {
            cx.try_global::<crate::fold_memory::FoldMemory>()
                .map(|memory| memory.get(path))
        })
        .unwrap_or_default()
}

/// Gives the text in `range` a background.
pub(crate) fn tint(runs: Vec<TextRun>, range: &Range<usize>, color: Hsla) -> Vec<TextRun> {
    restyle(runs, range, true, |part| {
        part.background_color = Some(color);
    })
}

fn fade_outside(runs: Vec<TextRun>, keep: &Range<usize>) -> Vec<TextRun> {
    restyle(runs, keep, false, |part| {
        part.color = part.color.opacity(DIMMED);
        part.background_color = part.background_color.map(|c| c.opacity(DIMMED));
        if let Some(underline) = &mut part.underline {
            underline.color = underline.color.map(|c| c.opacity(DIMMED));
        }
        if let Some(strike) = &mut part.strikethrough {
            strike.color = strike.color.map(|c| c.opacity(DIMMED));
        }
    })
}

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
        if let Some(vim) = &mut self.vim {
            vim.typed(text);
        }
        self.correct_finished_word(text, cx);
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
            Hit::Table(_) | Hit::Island(_) => None,
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
        self.pixel_scale = window.scale_factor() / gpui_kit::SMOOTH_SVG_SCALE_FACTOR;
        self.keep_revealing(window);
        self.keep_centering(window);
        self.offer_to_writing_tools(window, cx);
        let height = window.viewport_size().height;
        if self.typewriter_active() && self.padded_for != height {
            self.padded_for = height;
            self.remeasure();
        }
        self.painted.borrow_mut().clear();
        let entity = cx.entity();
        let focus = self.focus_handle.clone();
        let frame = self.frame.clone();
        let preview = self.render_footnote_preview(&theme);
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
            .font_family(self.typography.prose)
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::highlight))
            .on_action(cx.listener(Self::show_in_finder))
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
            .on_action(cx.listener(Self::strikethrough))
            .on_action(cx.listener(Self::inline_code))
            .on_action(cx.listener(Self::insert_link))
            .on_action(cx.listener(Self::open_link))
            .on_action(cx.listener(Self::export_html))
            .on_action(cx.listener(Self::export_pdf))
            .on_action(cx.listener(Self::print))
            .on_action(cx.listener(Self::copy_html))
            .on_action(cx.listener(Self::set_heading))
            .on_action(cx.listener(Self::toggle_bullets))
            .on_action(cx.listener(Self::toggle_numbers))
            .on_action(cx.listener(Self::toggle_task))
            .on_action(cx.listener(Self::toggle_quote))
            .on_action(cx.listener(Self::insert_table))
            .on_action(cx.listener(Self::insert_code_block))
            .on_action(cx.listener(Self::insert_math))
            .on_action(cx.listener(Self::toggle_focus_mode))
            .on_action(cx.listener(Self::show_character_palette))
            .on_action(cx.listener(Self::close_window))
            .on_action(cx.listener(Self::replace_word))
            .on_action(cx.listener(Self::ignore_spelling))
            .on_action(cx.listener(Self::learn_spelling))
            .on_action(cx.listener(Self::cell_exit))
            .on_action(cx.listener(Self::table_op))
            .on_action(cx.listener(Self::cell_next))
            .on_action(cx.listener(Self::cell_previous))
            .on_action(cx.listener(Self::cell_below))
            .on_action(cx.listener(Self::cell_up))
            .on_action(cx.listener(Self::cell_down))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_down(MouseButton::Right, cx.listener(Self::on_right_mouse_down))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .child(
                canvas(
                    |_, _, _| {},
                    move |bounds, (), window, cx| {
                        frame.set(bounds);
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
                    .text_color(if self.focus_mode {
                        theme.marker.opacity(DIMMED)
                    } else {
                        theme.marker
                    })
                    .bg(theme.background)
                    .window_control_area(WindowControlArea::Drag)
                    .child(title),
            )
            .children(banner)
            .children(preview)
            .children(self.render_vim_status(&theme))
    }
}
