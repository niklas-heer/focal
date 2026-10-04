//! Vim's modal editing as a state machine over the document's text: keys go
//! in; selections, edits and requests for the editor come out. The editor
//! stays the one place that changes text, so undo, autosave and
//! Markdown-aware typing work as they do without Vim.
//!
//! Offsets are byte offsets into the text, always on character boundaries.
//! In normal mode the cursor sits on a character, as in Vim; the editor
//! draws it as a block.

use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Insert,
    Visual,
    VisualLine,
}

impl Mode {
    /// The name Vim shows for the mode, or `None` for normal mode.
    pub const fn label(self) -> Option<&'static str> {
        match self {
            Self::Normal => None,
            Self::Insert => Some("INSERT"),
            Self::Visual => Some("VISUAL"),
            Self::VisualLine => Some("VISUAL LINE"),
        }
    }

    const fn visual(self) -> bool {
        matches!(self, Self::Visual | Self::VisualLine)
    }
}

/// A key as Vim reads it. Keys with ⌘ never reach Vim: they stay Focal's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Escape,
    Enter,
    Backspace,
    Tab,
    Ctrl(char),
}

/// What the editor knows when a key arrives.
#[derive(Clone, Copy, Debug)]
pub struct Context<'a> {
    pub text: &'a str,
    /// The selection's fixed end and its moving end; equal for a caret.
    pub anchor: usize,
    pub head: usize,
    /// The system clipboard's text, so `p` pastes what was copied elsewhere.
    pub clipboard: Option<&'a str>,
}

/// What Vim asks of the editor, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// Sets the selection; equal ends make a caret.
    Select {
        anchor: usize,
        head: usize,
    },
    /// Replaces `range` with `text`, then puts the caret at `caret`.
    Edit {
        range: Range<usize>,
        text: String,
        caret: usize,
    },
    /// Focal's own moves and edits, which know screen lines, tables and lists.
    Editor(EditorCommand),
    Undo,
    Redo,
    /// Puts yanked text on the system clipboard.
    Copy(String),
    /// Edits until [`Command::EndGroup`] undo as one step.
    BeginGroup,
    EndGroup,
    /// Opens the find bar, searching forward or backward.
    Find {
        forward: bool,
    },
    FindNext {
        forward: bool,
    },
    FindWord {
        word: String,
        forward: bool,
    },
    /// Replays the last change: these keys, then `text` typed if they end in
    /// insert mode. The editor feeds them back one by one.
    Repeat {
        keys: Vec<Key>,
        text: String,
    },
    Save,
    Close,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditorCommand {
    /// One screen line up or down, as the arrow keys move.
    Up,
    Down,
    /// A new line after the caret that continues a list, as Return does.
    Newline,
    /// Indents or outdents every line touching `range`, list items as Tab
    /// and ⇧Tab do.
    Indent {
        range: Range<usize>,
        outdent: bool,
    },
}

/// Yanked or deleted text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Register {
    pub(crate) text: String,
    pub(crate) linewise: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FindKind {
    /// `f`: onto the character.
    Forward,
    /// `F`
    Backward,
    /// `t`: up to the character.
    TillForward,
    /// `T`
    TillBackward,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Motion {
    Left,
    Right,
    /// `j`/`k` by lines of text; plain `j` and `k` move by screen lines.
    LineDown,
    LineUp,
    ScreenDown,
    ScreenUp,
    WordStart(bool),
    WordEnd(bool),
    WordBack(bool),
    WordEndBack(bool),
    LineStart,
    FirstNonBlank,
    /// `_`: the first non-blank, count-1 lines down; linewise.
    FirstNonBlankDown,
    LineEnd,
    /// `gg`/`G`: a numbered line, or the first or last line.
    GoToLine(Option<usize>),
    LastLine,
    ParagraphBack,
    ParagraphForward,
    Find(FindKind, char),
    RepeatFind(bool),
    MatchBracket,
    NextLine,
    PreviousLine,
    HalfPageDown,
    HalfPageUp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Object {
    Word {
        big: bool,
        around: bool,
    },
    Sentence {
        around: bool,
    },
    Paragraph {
        around: bool,
    },
    Quote {
        quote: char,
        around: bool,
    },
    Bracket {
        open: char,
        close: char,
        around: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Operator {
    Delete,
    Change,
    Yank,
    Indent,
    Outdent,
    Lower,
    Upper,
    ToggleCase,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Insert,
    Append,
    InsertAtStart,
    AppendAtEnd,
    OpenBelow,
    OpenAbove,
    DeleteChar,
    DeleteBack,
    Substitute,
    SubstituteLine,
    ChangeToEnd,
    DeleteToEnd,
    YankLine,
    Put {
        after: bool,
    },
    Undo,
    Redo,
    Repeat,
    Join,
    Replace(char),
    ToggleCase,
    Visual,
    VisualLine,
    CommandLine,
    Search {
        forward: bool,
    },
    SearchNext {
        reverse: bool,
    },
    SearchWord {
        forward: bool,
    },
    /// Visual mode: go to the other end.
    SwapEnds,
    Escape,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Target {
    Motion(Motion),
    Object(Object),
    /// `dd`, `cc`, `yy`, `>>`: the count's lines.
    Lines,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Parsed {
    Move(Motion),
    Operate(Operator, Target),
    /// An operator in visual mode, over the selection.
    OperateSelection(Operator),
    /// A text object in visual mode extends the selection over it.
    Select(Object),
    Act(Action),
}

enum Parse {
    Pending,
    Invalid,
    Done {
        count: Option<usize>,
        parsed: Parsed,
    },
}

/// A change `.` repeats: its keys and the text typed after them.
#[derive(Clone, Debug, Default)]
struct Change {
    keys: Vec<Key>,
    text: String,
}

#[derive(Debug)]
pub struct Vim {
    mode: Mode,
    /// Keys of the normal or visual command being typed.
    keys: Vec<Key>,
    /// Visual mode's fixed end and cursor, on characters.
    visual: (usize, usize),
    /// The selection Vim last set in visual mode, to notice the mouse.
    shown: Option<(usize, usize)>,
    register: Register,
    /// The clipboard's text when the register was last written: a different
    /// clipboard was copied since, and `p` pastes that instead.
    clipboard_then: Option<String>,
    last_find: Option<(FindKind, char)>,
    search_backward: bool,
    last_change: Option<Change>,
    recording: Option<Change>,
    replaying: bool,
    /// The column `j` and `k` keep in visual mode and under operators.
    goal: Option<usize>,
    command_line: Option<String>,
    message: Option<String>,
    /// Whether a change group is open until insert mode ends.
    grouped: bool,
}

impl Default for Vim {
    fn default() -> Self {
        Self::new()
    }
}

impl Vim {
    pub fn new() -> Self {
        Self {
            mode: Mode::Normal,
            keys: Vec::new(),
            visual: (0, 0),
            shown: None,
            register: Register::default(),
            clipboard_then: None,
            last_find: None,
            search_backward: false,
            last_change: None,
            recording: None,
            replaying: false,
            goal: None,
            command_line: None,
            message: None,
            grouped: false,
        }
    }

    pub const fn mode(&self) -> Mode {
        self.mode
    }

    /// What to show beside the mode: the command line, a message, or the
    /// keys of a command being typed.
    pub fn status(&self) -> Option<String> {
        if let Some(line) = &self.command_line {
            return Some(format!(":{line}"));
        }
        if let Some(message) = &self.message {
            return Some(message.clone());
        }
        (!self.keys.is_empty()).then(|| self.keys.iter().map(|key| key_name(*key)).collect())
    }

    /// Where the block cursor is drawn, if anywhere: on the caret in normal
    /// mode, on visual mode's cursor, nowhere while inserting.
    pub const fn block(&self, head: usize) -> Option<usize> {
        match self.mode {
            Mode::Normal => Some(head),
            Mode::Visual | Mode::VisualLine => Some(self.visual.1),
            Mode::Insert => None,
        }
    }

    /// Whether the editor is feeding back a repeated change.
    pub const fn set_replaying(&mut self, replaying: bool) {
        self.replaying = replaying;
    }

    /// Text typed in insert mode, for `.` to repeat.
    pub fn typed(&mut self, text: &str) {
        if let Some(change) = &mut self.recording {
            change.text.push_str(text);
        }
    }

    /// A backspace in insert mode, for `.` to repeat.
    pub fn backspaced(&mut self) {
        if let Some(change) = &mut self.recording {
            change.text.pop();
        }
    }

    /// Leaves insert or visual mode for normal mode, as when the pointer
    /// starts a new selection.
    pub fn reset(&mut self) {
        if self.mode.visual() {
            self.mode = Mode::Normal;
        }
        self.keys.clear();
        self.shown = None;
    }

    /// Where the caret belongs after Vim's commands in normal mode: on a
    /// character, never past a line's last one.
    pub fn settle(&self, text: &str, anchor: usize, head: usize) -> (usize, usize) {
        if self.mode == Mode::Normal {
            let at = normal_cursor(text, head.min(anchor.max(head)));
            (at, at)
        } else {
            (anchor, head)
        }
    }

    /// Handles a key. `None` leaves it to the editor, as for typing in
    /// insert mode and keys Vim has no use for.
    pub fn key(&mut self, key: Key, cx: &Context) -> Option<Vec<Command>> {
        self.message = None;
        if self.command_line.is_some() {
            return Some(self.command_line_key(key, cx));
        }
        if self.mode == Mode::Insert {
            return self.insert_key(key, cx);
        }
        if matches!(key, Key::Tab) {
            return Some(Vec::new());
        }
        if let Key::Ctrl(c) = key
            && !matches!(c, 'r' | 'd' | 'u' | '[' | 'c')
        {
            return None;
        }
        // A selection made with the pointer in normal mode becomes visual.
        if self.mode == Mode::Normal && cx.anchor != cx.head {
            self.mode = Mode::Visual;
            let (lo, hi) = (cx.anchor.min(cx.head), cx.anchor.max(cx.head));
            let last = prev_char(cx.text, hi).max(lo);
            self.visual = if cx.head < cx.anchor {
                (last, cx.head)
            } else {
                (lo, last)
            };
            self.shown = Some((cx.anchor, cx.head));
        }
        if self.mode.visual() && self.shown != Some((cx.anchor, cx.head)) {
            self.resync_visual(cx);
        }
        self.keys.push(key);
        let parse = parse(&self.keys, self.mode.visual());
        match parse {
            Parse::Pending => Some(Vec::new()),
            Parse::Invalid => {
                self.keys.clear();
                Some(Vec::new())
            }
            Parse::Done { count, parsed } => {
                let keys = std::mem::take(&mut self.keys);
                let head = if self.mode == Mode::Normal {
                    normal_cursor(cx.text, cx.head)
                } else {
                    cx.head
                };
                let changes = changes_text(parsed);
                if changes && !self.replaying && self.mode == Mode::Normal {
                    self.recording = Some(Change {
                        keys,
                        text: String::new(),
                    });
                }
                let commands = self.run(parsed, count, head, cx);
                if self.mode != Mode::Insert && self.recording.is_some() {
                    self.last_change = self.recording.take();
                }
                Some(commands)
            }
        }
    }

    /// Visual mode's ends again after the pointer changed the selection.
    fn resync_visual(&mut self, cx: &Context) {
        self.visual = match cx.head.cmp(&cx.anchor) {
            std::cmp::Ordering::Equal => (cx.head, cx.head),
            std::cmp::Ordering::Greater => (cx.anchor, prev_char(cx.text, cx.head).max(cx.anchor)),
            std::cmp::Ordering::Less => (prev_char(cx.text, cx.anchor).max(cx.head), cx.head),
        };
    }

    fn insert_key(&mut self, key: Key, cx: &Context) -> Option<Vec<Command>> {
        if !matches!(key, Key::Escape | Key::Ctrl('[' | 'c')) {
            return None;
        }
        self.mode = Mode::Normal;
        let mut commands = Vec::new();
        if self.grouped {
            self.grouped = false;
            commands.push(Command::EndGroup);
        }
        if self.recording.is_some() {
            self.last_change = self.recording.take();
        }
        // The cursor steps back onto the last typed character.
        let start = line_start(cx.text, cx.head);
        let at = if cx.head > start {
            prev_char(cx.text, cx.head)
        } else {
            cx.head
        };
        commands.push(caret(at));
        Some(commands)
    }

    fn command_line_key(&mut self, key: Key, cx: &Context) -> Vec<Command> {
        let Some(line) = self.command_line.as_mut() else {
            return Vec::new();
        };
        match key {
            Key::Char(c) => line.push(c),
            Key::Backspace => {
                if line.pop().is_none() {
                    self.command_line = None;
                }
            }
            Key::Enter => {
                let line = self.command_line.take().unwrap_or_default();
                return self.ex(line.trim(), cx);
            }
            Key::Escape | Key::Ctrl('[' | 'c') => self.command_line = None,
            _ => {}
        }
        Vec::new()
    }

    /// Runs an Ex command typed after `:`.
    fn ex(&mut self, line: &str, cx: &Context) -> Vec<Command> {
        match line {
            "w" | "w!" | "up" | "update" => vec![Command::Save],
            "q" | "q!" | "quit" | "close" => vec![Command::Close],
            "wq" | "wq!" | "x" | "xit" => vec![Command::Save, Command::Close],
            "" | "noh" | "nohlsearch" => Vec::new(),
            _ => {
                if let Ok(number) = line.parse::<usize>() {
                    let at =
                        first_non_blank(cx.text, line_offset(cx.text, number.saturating_sub(1)));
                    return vec![caret(at)];
                }
                self.message = Some(format!("Not an editor command: {line}"));
                Vec::new()
            }
        }
    }

    fn run(
        &mut self,
        parsed: Parsed,
        count: Option<usize>,
        head: usize,
        cx: &Context,
    ) -> Vec<Command> {
        if !matches!(parsed, Parsed::Move(Motion::LineDown | Motion::LineUp)) {
            self.goal = None;
        }
        match parsed {
            Parsed::Move(motion) => self.motion_command(motion, count, head, cx),
            Parsed::Operate(operator, target) => {
                let Some((range, linewise)) = self.target_range(target, operator, count, head, cx)
                else {
                    return Vec::new();
                };
                self.operate(operator, range, linewise, head, cx)
            }
            Parsed::OperateSelection(operator) => {
                let (range, linewise) = self.visual_range(cx.text);
                let head = range.start;
                self.mode = Mode::Normal;
                self.shown = None;
                self.operate(operator, range, linewise, head, cx)
            }
            Parsed::Select(object) => {
                let Some((range, linewise)) = object_range(object, cx.text, self.visual.1) else {
                    return Vec::new();
                };
                if linewise && self.mode == Mode::Visual {
                    self.mode = Mode::VisualLine;
                }
                if range.is_empty() {
                    return Vec::new();
                }
                self.visual = (range.start, prev_char(cx.text, range.end));
                vec![self.show_visual(cx.text)]
            }
            Parsed::Act(action) => self.act(action, count, head, cx),
        }
    }

    /// A motion on its own: moves the caret, or visual mode's cursor.
    fn motion_command(
        &mut self,
        motion: Motion,
        count: Option<usize>,
        head: usize,
        cx: &Context,
    ) -> Vec<Command> {
        let n = count.unwrap_or(1);
        if self.mode == Mode::Normal {
            let screen = match motion {
                Motion::ScreenDown | Motion::LineDown => Some((EditorCommand::Down, n)),
                Motion::ScreenUp | Motion::LineUp => Some((EditorCommand::Up, n)),
                Motion::HalfPageDown => Some((EditorCommand::Down, HALF_PAGE)),
                Motion::HalfPageUp => Some((EditorCommand::Up, HALF_PAGE)),
                _ => None,
            };
            if let Some((command, times)) = screen {
                return vec![Command::Editor(command); times];
            }
        }
        let from = if self.mode.visual() {
            self.visual.1
        } else {
            head
        };
        let Some(to) = self.motion(motion, count, from, cx.text, false) else {
            return Vec::new();
        };
        if self.mode.visual() {
            // Visual mode's cursor sits on a character, even after `$`.
            let at = if to.at >= cx.text.len() && !cx.text.is_empty() {
                prev_char(cx.text, cx.text.len())
            } else {
                to.at
            };
            self.visual.1 = at;
            vec![self.show_visual(cx.text)]
        } else {
            vec![caret(normal_cursor(cx.text, to.at))]
        }
    }

    /// The selection that shows visual mode's range, remembered so that a
    /// change made with the pointer is noticed.
    fn show_visual(&mut self, text: &str) -> Command {
        let (range, _) = self.visual_range(text);
        let (anchor, head) = if self.visual.1 < self.visual.0 {
            (range.end, range.start)
        } else {
            (range.start, range.end)
        };
        self.shown = Some((anchor, head));
        Command::Select { anchor, head }
    }

    /// Visual mode's range, inclusive of the cursor's character, and whether
    /// it is whole lines.
    fn visual_range(&self, text: &str) -> (Range<usize>, bool) {
        let (lo, hi) = (
            self.visual.0.min(self.visual.1),
            self.visual.0.max(self.visual.1),
        );
        if self.mode == Mode::VisualLine {
            (line_start(text, lo)..line_end_with_newline(text, hi), true)
        } else {
            (lo..next_char(text, hi), false)
        }
    }

    /// Where a motion lands from `from`. `operating` notes that an operator
    /// waits for it, which changes `w` at a line's end.
    fn motion(
        &mut self,
        motion: Motion,
        count: Option<usize>,
        from: usize,
        text: &str,
        operating: bool,
    ) -> Option<Landing> {
        let n = count.unwrap_or(1).max(1);
        let landing = |at: usize| Some(Landing::exclusive(at));
        match motion {
            Motion::Left => landing(repeat(n, from, |at| {
                (at > line_start(text, at)).then(|| prev_char(text, at))
            })),
            Motion::Right => {
                let end = line_end(text, from);
                let at = repeat(n, from, |at| (at < end).then(|| next_char(text, at)));
                // Without an operator, `l` stops on the last character.
                landing(if operating {
                    at
                } else {
                    at.min(prev_char(text, end).max(line_start(text, from)))
                })
            }
            Motion::LineDown | Motion::LineUp | Motion::ScreenDown | Motion::ScreenUp => {
                let down = matches!(motion, Motion::LineDown | Motion::ScreenDown);
                self.vertical(down, n, from, text)
            }
            Motion::WordStart(big) => {
                let mut at = from;
                for ix in 0..n {
                    let next = word_start(text, at, big);
                    // `dw` on a line's last word stops at its end.
                    if operating && ix + 1 == n && line_of(text, next) != line_of(text, at) {
                        let end = line_end(text, at);
                        at = if end > at { end } else { next };
                    } else {
                        at = next;
                    }
                }
                landing(at)
            }
            Motion::WordEnd(big) => Some(Landing::inclusive(repeat(n, from, |at| {
                Some(word_end(text, at, big))
            }))),
            Motion::WordBack(big) => landing(repeat(n, from, |at| Some(word_back(text, at, big)))),
            Motion::WordEndBack(big) => Some(Landing::inclusive(repeat(n, from, |at| {
                Some(word_end_back(text, at, big))
            }))),
            Motion::LineStart => landing(line_start(text, from)),
            Motion::FirstNonBlank => landing(first_non_blank(text, line_start(text, from))),
            Motion::LineEnd => {
                let line = line_offset(text, line_of(text, from) + n - 1);
                let end = line_end(text, line);
                // An operator takes the last character, if there is one.
                Some(if operating && end > line {
                    Landing::inclusive(prev_char(text, end))
                } else {
                    Landing::exclusive(end)
                })
            }
            Motion::FirstNonBlankDown
            | Motion::GoToLine(_)
            | Motion::LastLine
            | Motion::NextLine
            | Motion::PreviousLine
            | Motion::HalfPageDown
            | Motion::HalfPageUp => Some(line_motion(motion, count, from, text)),
            Motion::ParagraphForward => {
                landing(repeat(n, from, |at| Some(paragraph_forward(text, at))))
            }
            Motion::ParagraphBack => landing(repeat(n, from, |at| Some(paragraph_back(text, at)))),
            Motion::Find(kind, c) => {
                self.last_find = Some((kind, c));
                find_in_line(text, from, kind, c, n, false)
            }
            Motion::RepeatFind(reverse) => {
                let (kind, c) = self.last_find?;
                let kind = if reverse { reversed(kind) } else { kind };
                find_in_line(text, from, kind, c, n, true)
            }
            Motion::MatchBracket => match_bracket(text, from).map(Landing::inclusive),
        }
    }

    /// `j` and `k` by lines of text, keeping the column; nowhere past the
    /// first or last line.
    fn vertical(&mut self, down: bool, n: usize, from: usize, text: &str) -> Option<Landing> {
        let goal = *self.goal.get_or_insert_with(|| column(text, from));
        let line = line_of(text, from);
        let last = line_count(text) - 1;
        let target = if down {
            (line + n).min(last)
        } else {
            line.saturating_sub(n)
        };
        (target != line).then(|| Landing {
            at: at_column(text, line_offset(text, target), goal),
            linewise: true,
            inclusive: false,
        })
    }

    /// The range an operator works on, and whether it is whole lines.
    fn target_range(
        &mut self,
        target: Target,
        operator: Operator,
        count: Option<usize>,
        head: usize,
        cx: &Context,
    ) -> Option<(Range<usize>, bool)> {
        let text = cx.text;
        match target {
            Target::Lines => {
                let first = line_of(text, head);
                let last = first + count.unwrap_or(1).max(1) - 1;
                let start = line_start(text, head);
                let end = line_end_with_newline(text, line_offset(text, last));
                Some((start..end, true))
            }
            Target::Object(object) => object_range(object, text, head),
            Target::Motion(motion) => {
                // `cw` changes to the word's end, as `ce` does.
                let motion = match motion {
                    Motion::WordStart(big)
                        if operator == Operator::Change
                            && char_at(text, head).is_some_and(|c| !c.is_whitespace()) =>
                    {
                        Motion::WordEnd(big)
                    }
                    other => other,
                };
                let to = self.motion(motion, count, head, text, true)?;
                if to.linewise {
                    let (lo, hi) = (head.min(to.at), head.max(to.at));
                    Some((line_start(text, lo)..line_end_with_newline(text, hi), true))
                } else {
                    let (lo, hi) = (head.min(to.at), head.max(to.at));
                    let hi = if to.inclusive {
                        next_char(text, hi)
                    } else {
                        hi
                    };
                    Some((lo..hi, false))
                }
            }
        }
    }

    fn write_register(&mut self, mut text: String, linewise: bool, cx: &Context) {
        // Lines end with a line ending, the document's last line too.
        if linewise && !text.ends_with('\n') {
            text.push_str(line_ending(cx.text));
        }
        self.clipboard_then = cx.clipboard.map(str::to_owned);
        self.register = Register { text, linewise };
    }

    /// The register `p` pastes: a clipboard copied since Vim last wrote its
    /// register, or the register.
    fn paste_register(&self, cx: &Context) -> Register {
        match cx.clipboard {
            Some(clip) if self.clipboard_then.as_deref() != Some(clip) => Register {
                text: clip.to_owned(),
                linewise: clip.ends_with('\n'),
            },
            _ => self.register.clone(),
        }
    }

    fn begin_change(&mut self, commands: &mut Vec<Command>) {
        if !self.grouped {
            self.grouped = true;
            commands.push(Command::BeginGroup);
        }
    }

    fn operate(
        &mut self,
        operator: Operator,
        range: Range<usize>,
        linewise: bool,
        head: usize,
        cx: &Context,
    ) -> Vec<Command> {
        let text = cx.text;
        let mut commands = Vec::new();
        match operator {
            Operator::Delete => {
                let range = if linewise {
                    whole_lines(text, range)
                } else {
                    range
                };
                self.write_register(text[range.clone()].to_owned(), linewise, cx);
                let after = edited(text, &range, "");
                let at = if linewise {
                    first_non_blank(&after, line_start(&after, range.start.min(after.len())))
                } else {
                    range.start
                };
                commands.push(Command::Edit {
                    range,
                    text: String::new(),
                    caret: at,
                });
            }
            Operator::Change => {
                self.write_register(text[range.clone()].to_owned(), linewise, cx);
                // Lines keep their indentation and their last line ending.
                let range = if linewise {
                    let start = first_non_blank(text, range.start);
                    let end = line_end(text, prev_char(text, range.end).max(range.start));
                    start.min(end)..end
                } else {
                    range
                };
                self.begin_change(&mut commands);
                commands.push(Command::Edit {
                    caret: range.start,
                    range,
                    text: String::new(),
                });
                self.mode = Mode::Insert;
            }
            Operator::Yank => {
                self.write_register(text[range.clone()].to_owned(), linewise, cx);
                let yanked = self.register.text.clone();
                self.clipboard_then = Some(yanked.clone());
                commands.push(Command::Copy(yanked));
                let at = if linewise {
                    head.min(range.end)
                } else {
                    range.start
                };
                commands.push(caret(at));
            }
            Operator::Indent | Operator::Outdent => {
                commands.push(Command::Editor(EditorCommand::Indent {
                    range: range.clone(),
                    outdent: operator == Operator::Outdent,
                }));
            }
            Operator::Lower | Operator::Upper | Operator::ToggleCase => {
                let changed: String = text[range.clone()]
                    .chars()
                    .flat_map(|c| -> Vec<char> {
                        match operator {
                            Operator::Lower => c.to_lowercase().collect(),
                            Operator::Upper => c.to_uppercase().collect(),
                            _ => toggle_case(c),
                        }
                    })
                    .collect();
                commands.push(Command::Edit {
                    caret: range.start,
                    range,
                    text: changed,
                });
            }
        }
        commands
    }

    #[allow(clippy::too_many_lines)]
    fn act(
        &mut self,
        action: Action,
        count: Option<usize>,
        head: usize,
        cx: &Context,
    ) -> Vec<Command> {
        let text = cx.text;
        let n = count.unwrap_or(1).max(1);
        let mut commands = Vec::new();
        match action {
            Action::Escape => {
                if self.mode.visual() {
                    self.mode = Mode::Normal;
                    self.shown = None;
                    commands.push(caret(self.visual.1));
                }
            }
            Action::Insert | Action::Append | Action::InsertAtStart | Action::AppendAtEnd => {
                let at = match action {
                    Action::Insert => head,
                    Action::Append => {
                        if head < line_end(text, head) {
                            next_char(text, head)
                        } else {
                            head
                        }
                    }
                    Action::InsertAtStart => first_non_blank(text, line_start(text, head)),
                    _ => line_end(text, head),
                };
                self.begin_change(&mut commands);
                commands.push(caret(at));
                self.mode = Mode::Insert;
            }
            Action::OpenBelow => {
                self.begin_change(&mut commands);
                commands.push(caret(line_end(text, head)));
                commands.push(Command::Editor(EditorCommand::Newline));
                self.mode = Mode::Insert;
            }
            Action::OpenAbove => {
                let start = line_start(text, head);
                self.begin_change(&mut commands);
                commands.push(Command::Edit {
                    range: start..start,
                    text: line_ending(text).to_owned(),
                    caret: start,
                });
                self.mode = Mode::Insert;
            }
            Action::DeleteChar | Action::Substitute => {
                let end = line_end(text, head);
                let to = repeat(n, head, |at| (at < end).then(|| next_char(text, at)));
                if action == Action::Substitute {
                    return self.operate(Operator::Change, head..to, false, head, cx);
                }
                if to > head {
                    return self.operate(Operator::Delete, head..to, false, head, cx);
                }
            }
            Action::DeleteBack => {
                let start = line_start(text, head);
                let from = repeat(n, head, |at| (at > start).then(|| prev_char(text, at)));
                if from < head {
                    return self.operate(Operator::Delete, from..head, false, head, cx);
                }
            }
            Action::SubstituteLine => {
                return self.operate_target(Operator::Change, Target::Lines, count, head, cx);
            }
            Action::ChangeToEnd | Action::DeleteToEnd => {
                let operator = if action == Action::ChangeToEnd {
                    Operator::Change
                } else {
                    Operator::Delete
                };
                let end = line_end(text, line_offset(text, line_of(text, head) + n - 1));
                return self.operate(operator, head..end.max(head), false, head, cx);
            }
            Action::YankLine => {
                return self.operate_target(Operator::Yank, Target::Lines, count, head, cx);
            }
            Action::Put { after } => commands.extend(self.put(after, n, head, cx)),
            Action::Undo => commands.extend(std::iter::repeat_n(Command::Undo, n)),
            Action::Redo => commands.extend(std::iter::repeat_n(Command::Redo, n)),
            Action::Repeat => {
                if let Some(change) = &self.last_change {
                    commands.push(Command::Repeat {
                        keys: change.keys.clone(),
                        text: change.text.clone(),
                    });
                }
            }
            Action::Join => {
                if self.mode.visual() {
                    let (range, _) = self.visual_range(text);
                    self.mode = Mode::Normal;
                    self.shown = None;
                    let lines = line_of(text, prev_char(text, range.end).max(range.start))
                        - line_of(text, range.start)
                        + 1;
                    commands.extend(join(text, range.start, lines.max(2)));
                } else {
                    commands.extend(join(text, head, n.max(2)));
                }
            }
            Action::Replace(c) => {
                if self.mode.visual() {
                    let (range, _) = self.visual_range(text);
                    self.mode = Mode::Normal;
                    self.shown = None;
                    let replaced: String = text[range.clone()]
                        .chars()
                        .map(|old| if old == '\n' || old == '\r' { old } else { c })
                        .collect();
                    commands.push(Command::Edit {
                        caret: range.start,
                        range,
                        text: replaced,
                    });
                    return commands;
                }
                let end = line_end(text, head);
                let mut to = head;
                let mut chars = 0;
                while chars < n && to < end {
                    to = next_char(text, to);
                    chars += 1;
                }
                if chars == n {
                    let (new, caret) = if c == '\n' {
                        (line_ending(text).to_owned(), head + line_ending(text).len())
                    } else {
                        let new = c.to_string().repeat(n);
                        let last = head + new.len() - c.len_utf8();
                        (new, last)
                    };
                    commands.push(Command::Edit {
                        range: head..to,
                        text: new,
                        caret,
                    });
                }
            }
            Action::ToggleCase => {
                if self.mode.visual() {
                    return self.act_selection(Operator::ToggleCase, cx);
                }
                let end = line_end(text, head);
                let to = repeat(n, head, |at| (at < end).then(|| next_char(text, at)));
                if to > head {
                    let changed: String = text[head..to].chars().flat_map(toggle_case).collect();
                    let caret = if to < end {
                        head + changed.len()
                    } else {
                        head + changed.len()
                            - text[..to].chars().next_back().map_or(0, char::len_utf8)
                    };
                    commands.push(Command::Edit {
                        range: head..to,
                        text: changed,
                        caret,
                    });
                }
            }
            Action::Visual | Action::VisualLine => {
                let mode = if action == Action::Visual {
                    Mode::Visual
                } else {
                    Mode::VisualLine
                };
                if self.mode == mode {
                    self.mode = Mode::Normal;
                    self.shown = None;
                    commands.push(caret(self.visual.1));
                } else {
                    if self.mode == Mode::Normal {
                        self.visual = (head, head);
                    }
                    self.mode = mode;
                    commands.push(self.show_visual(text));
                }
            }
            Action::SwapEnds => {
                self.visual = (self.visual.1, self.visual.0);
                commands.push(self.show_visual(text));
            }
            Action::CommandLine => self.command_line = Some(String::new()),
            Action::Search { forward } => {
                self.search_backward = !forward;
                commands.push(Command::Find { forward });
            }
            Action::SearchNext { reverse } => commands.push(Command::FindNext {
                forward: self.search_backward == reverse,
            }),
            Action::SearchWord { forward } => {
                let range = object_range(
                    Object::Word {
                        big: false,
                        around: false,
                    },
                    text,
                    head,
                )
                .map(|(range, _)| range);
                if let Some(range) = range.filter(|range| {
                    text[range.clone()]
                        .chars()
                        .any(|c| class(c, false) == Class::Word)
                }) {
                    self.search_backward = !forward;
                    commands.push(Command::FindWord {
                        word: text[range].to_owned(),
                        forward,
                    });
                }
            }
        }
        commands
    }

    fn operate_target(
        &mut self,
        operator: Operator,
        target: Target,
        count: Option<usize>,
        head: usize,
        cx: &Context,
    ) -> Vec<Command> {
        match self.target_range(target, operator, count, head, cx) {
            Some((range, linewise)) => self.operate(operator, range, linewise, head, cx),
            None => Vec::new(),
        }
    }

    fn act_selection(&mut self, operator: Operator, cx: &Context) -> Vec<Command> {
        let (range, linewise) = self.visual_range(cx.text);
        self.mode = Mode::Normal;
        self.shown = None;
        self.operate(operator, range.clone(), linewise, range.start, cx)
    }

    fn put(&mut self, after: bool, n: usize, head: usize, cx: &Context) -> Vec<Command> {
        let text = cx.text;
        let register = self.paste_register(cx);
        if register.text.is_empty() {
            return Vec::new();
        }
        if self.mode.visual() {
            let (range, _) = self.visual_range(text);
            self.mode = Mode::Normal;
            self.shown = None;
            let new = register.text.repeat(n);
            let caret = prev_char(&new, new.len()) + range.start;
            return vec![Command::Edit {
                range,
                text: new,
                caret,
            }];
        }
        if register.linewise {
            let mut lines = register.text.repeat(n);
            let at = if after {
                line_end_with_newline(text, head)
            } else {
                line_start(text, head)
            };
            // After a last line without a line ending, the pasted lines need one first.
            let (at, new) =
                if after && at == text.len() && !text.is_empty() && !text.ends_with('\n') {
                    if lines.ends_with('\n') {
                        lines.pop();
                        if lines.ends_with('\r') {
                            lines.pop();
                        }
                    }
                    let ending = line_ending(text);
                    (at, format!("{ending}{lines}"))
                } else {
                    (at, lines)
                };
            let start = if new.starts_with(['\n', '\r']) {
                at + line_ending(text).len()
            } else {
                at
            };
            let caret = start + (first_non_blank(&new[start - at..], 0));
            vec![Command::Edit {
                range: at..at,
                text: new,
                caret,
            }]
        } else {
            let new = register.text.repeat(n);
            let at = if after && head < line_end(text, head) {
                next_char(text, head)
            } else {
                head
            };
            let caret = at + prev_char(&new, new.len());
            vec![Command::Edit {
                range: at..at,
                text: new,
                caret,
            }]
        }
    }
}

/// Lines `j` and `k` move for ⌃D and ⌃U.
pub(crate) const HALF_PAGE: usize = 15;

/// Where a motion to a numbered or nearby line lands: its first non-blank.
fn line_motion(motion: Motion, count: Option<usize>, from: usize, text: &str) -> Landing {
    let n = count.unwrap_or(1).max(1);
    let line = line_of(text, from);
    let last = line_count(text) - 1;
    let target = match motion {
        Motion::GoToLine(line) => line.unwrap_or(n) - 1,
        Motion::LastLine => count.map_or(last, |n| n.max(1) - 1),
        Motion::NextLine => line + n,
        Motion::PreviousLine => line.saturating_sub(n),
        Motion::HalfPageDown => line + HALF_PAGE,
        Motion::HalfPageUp => line.saturating_sub(HALF_PAGE),
        _ => line + n - 1,
    };
    Landing::lines(first_non_blank(text, line_offset(text, target.min(last))))
}

/// Where a motion lands.
#[derive(Clone, Copy, Debug)]
struct Landing {
    at: usize,
    /// Operators take whole lines.
    linewise: bool,
    /// Operators include the character landed on.
    inclusive: bool,
}

impl Landing {
    const fn exclusive(at: usize) -> Self {
        Self {
            at,
            linewise: false,
            inclusive: false,
        }
    }

    const fn inclusive(at: usize) -> Self {
        Self {
            at,
            linewise: false,
            inclusive: true,
        }
    }

    const fn lines(at: usize) -> Self {
        Self {
            at,
            linewise: true,
            inclusive: false,
        }
    }
}

pub(crate) fn caret(at: usize) -> Command {
    Command::Select {
        anchor: at,
        head: at,
    }
}

/// Whether a command changes the text, so `.` repeats it.
const fn changes_text(parsed: Parsed) -> bool {
    match parsed {
        Parsed::Operate(operator, _) => !matches!(operator, Operator::Yank),
        Parsed::Act(action) => matches!(
            action,
            Action::Insert
                | Action::Append
                | Action::InsertAtStart
                | Action::AppendAtEnd
                | Action::OpenBelow
                | Action::OpenAbove
                | Action::DeleteChar
                | Action::DeleteBack
                | Action::Substitute
                | Action::SubstituteLine
                | Action::ChangeToEnd
                | Action::DeleteToEnd
                | Action::Put { .. }
                | Action::Join
                | Action::Replace(_)
                | Action::ToggleCase
        ),
        Parsed::Move(_) | Parsed::OperateSelection(_) | Parsed::Select(_) => false,
    }
}

pub(crate) fn key_name(key: Key) -> String {
    match key {
        Key::Char(c) => c.to_string(),
        Key::Escape => "⎋".to_owned(),
        Key::Enter => "↩".to_owned(),
        Key::Backspace => "⌫".to_owned(),
        Key::Tab => "⇥".to_owned(),
        Key::Ctrl(c) => format!("^{}", c.to_ascii_uppercase()),
    }
}

// ---- Parsing --------------------------------------------------------------

/// Reads `keys` as a command: a count, then an operator with a motion or
/// text object, a motion, or an action.
fn parse(keys: &[Key], visual: bool) -> Parse {
    let (count, rest) = read_count(keys);
    let Some(&first) = rest.first() else {
        return Parse::Pending;
    };
    let done = |parsed| Parse::Done { count, parsed };
    if let Some((operator, used)) = read_operator(rest) {
        if used > rest.len() {
            return Parse::Pending;
        }
        if visual {
            return done(Parsed::OperateSelection(operator));
        }
        let after = &rest[used..];
        let (second, after) = read_count(after);
        let count = match (count, second) {
            (Some(a), Some(b)) => Some(a * b),
            (a, b) => a.or(b),
        };
        let Some(&key) = after.first() else {
            return Parse::Pending;
        };
        // The operator's own last key again: `dd`, `cc`, `yy`, `>>`, `guu`.
        if Some(key) == rest.get(used - 1).copied() {
            return Parse::Done {
                count,
                parsed: Parsed::Operate(operator, Target::Lines),
            };
        }
        if let Key::Char(around @ ('i' | 'a')) = key {
            return match after.get(1) {
                None => Parse::Pending,
                Some(&key) => {
                    read_object(key, around == 'a').map_or(Parse::Invalid, |object| Parse::Done {
                        count,
                        parsed: Parsed::Operate(operator, Target::Object(object)),
                    })
                }
            };
        }
        return match read_motion(after) {
            Ok(Some((motion, _))) => Parse::Done {
                count,
                parsed: Parsed::Operate(operator, Target::Motion(motion)),
            },
            Ok(None) => Parse::Pending,
            Err(()) => Parse::Invalid,
        };
    }
    if visual && let Key::Char(around @ ('i' | 'a')) = first {
        return match rest.get(1) {
            None => Parse::Pending,
            Some(&key) => read_object(key, around == 'a')
                .map_or(Parse::Invalid, |object| done(Parsed::Select(object))),
        };
    }
    match read_motion(rest) {
        Ok(Some((motion, _))) => return done(Parsed::Move(motion)),
        Ok(None) => return Parse::Pending,
        Err(()) => {}
    }
    match read_action(rest, visual) {
        Ok(Some(action)) => match action {
            ActionOrOperator::Action(action) => done(Parsed::Act(action)),
            ActionOrOperator::Operator(operator) => done(Parsed::OperateSelection(operator)),
        },
        Ok(None) => Parse::Pending,
        Err(()) => Parse::Invalid,
    }
}

fn read_count(keys: &[Key]) -> (Option<usize>, &[Key]) {
    let mut count: Option<usize> = None;
    let mut used = 0;
    for key in keys {
        match key {
            Key::Char(c @ '0'..='9') if *c != '0' || count.is_some() => {
                let digit = c.to_digit(10).unwrap_or(0) as usize;
                count = Some(count.unwrap_or(0).saturating_mul(10).saturating_add(digit));
                used += 1;
            }
            _ => break,
        }
    }
    (count, &keys[used..])
}

/// An operator and how many keys it took; more than `keys` holds when it
/// needs another.
fn read_operator(keys: &[Key]) -> Option<(Operator, usize)> {
    match keys.first()? {
        Key::Char('d') => Some((Operator::Delete, 1)),
        Key::Char('c') => Some((Operator::Change, 1)),
        Key::Char('y') => Some((Operator::Yank, 1)),
        Key::Char('>') => Some((Operator::Indent, 1)),
        Key::Char('<') => Some((Operator::Outdent, 1)),
        Key::Char('g') => match keys.get(1) {
            Some(Key::Char('u')) => Some((Operator::Lower, 2)),
            Some(Key::Char('U')) => Some((Operator::Upper, 2)),
            Some(Key::Char('~')) => Some((Operator::ToggleCase, 2)),
            _ => None,
        },
        _ => None,
    }
}

pub(crate) fn read_object(key: Key, around: bool) -> Option<Object> {
    let Key::Char(c) = key else { return None };
    Some(match c {
        'w' => Object::Word { big: false, around },
        'W' => Object::Word { big: true, around },
        's' => Object::Sentence { around },
        'p' => Object::Paragraph { around },
        '"' | '\'' | '`' => Object::Quote { quote: c, around },
        '(' | ')' | 'b' => Object::Bracket {
            open: '(',
            close: ')',
            around,
        },
        '[' | ']' => Object::Bracket {
            open: '[',
            close: ']',
            around,
        },
        '{' | '}' | 'B' => Object::Bracket {
            open: '{',
            close: '}',
            around,
        },
        '<' | '>' => Object::Bracket {
            open: '<',
            close: '>',
            around,
        },
        _ => return None,
    })
}

/// A motion at the start of `keys`: `Ok(None)` while it needs more keys,
/// `Err` when the keys are no motion.
fn read_motion(keys: &[Key]) -> Result<Option<(Motion, usize)>, ()> {
    let Some(&first) = keys.first() else {
        return Ok(None);
    };
    let one = |motion| Ok(Some((motion, 1)));
    match first {
        Key::Char('h') | Key::Backspace => one(Motion::Left),
        Key::Char('l' | ' ') => one(Motion::Right),
        Key::Char('j') => one(Motion::LineDown),
        Key::Char('k') => one(Motion::LineUp),
        Key::Char('w') => one(Motion::WordStart(false)),
        Key::Char('W') => one(Motion::WordStart(true)),
        Key::Char('e') => one(Motion::WordEnd(false)),
        Key::Char('E') => one(Motion::WordEnd(true)),
        Key::Char('b') => one(Motion::WordBack(false)),
        Key::Char('B') => one(Motion::WordBack(true)),
        Key::Char('0') => one(Motion::LineStart),
        Key::Char('^') => one(Motion::FirstNonBlank),
        Key::Char('_') => one(Motion::FirstNonBlankDown),
        Key::Char('$') => one(Motion::LineEnd),
        Key::Char('G') => one(Motion::LastLine),
        Key::Char('{') => one(Motion::ParagraphBack),
        Key::Char('}') => one(Motion::ParagraphForward),
        Key::Char(';') => one(Motion::RepeatFind(false)),
        Key::Char(',') => one(Motion::RepeatFind(true)),
        Key::Char('%') => one(Motion::MatchBracket),
        Key::Char('+') | Key::Enter => one(Motion::NextLine),
        Key::Char('-') => one(Motion::PreviousLine),
        Key::Ctrl('d') => one(Motion::HalfPageDown),
        Key::Ctrl('u') => one(Motion::HalfPageUp),
        Key::Char(c @ ('f' | 'F' | 't' | 'T')) => {
            let kind = match c {
                'f' => FindKind::Forward,
                'F' => FindKind::Backward,
                't' => FindKind::TillForward,
                _ => FindKind::TillBackward,
            };
            match keys.get(1) {
                None => Ok(None),
                Some(Key::Char(target)) => Ok(Some((Motion::Find(kind, *target), 2))),
                Some(_) => Err(()),
            }
        }
        Key::Char('g') => match keys.get(1) {
            None => Ok(None),
            Some(Key::Char('g')) => Ok(Some((Motion::GoToLine(None), 2))),
            Some(Key::Char('j')) => Ok(Some((Motion::ScreenDown, 2))),
            Some(Key::Char('k')) => Ok(Some((Motion::ScreenUp, 2))),
            Some(Key::Char('e')) => Ok(Some((Motion::WordEndBack(false), 2))),
            Some(Key::Char('E')) => Ok(Some((Motion::WordEndBack(true), 2))),
            Some(Key::Char('_')) => Ok(Some((Motion::LineEnd, 2))),
            Some(_) => Err(()),
        },
        _ => Err(()),
    }
}

enum ActionOrOperator {
    Action(Action),
    /// Visual mode's one-key operators: `x`, `s`, `u`, `U`, `~`.
    Operator(Operator),
}

fn read_action(keys: &[Key], visual: bool) -> Result<Option<ActionOrOperator>, ()> {
    use ActionOrOperator::{Action as A, Operator as O};
    let Some(&first) = keys.first() else {
        return Ok(None);
    };
    let action = |action| Ok(Some(A(action)));
    if visual {
        match first {
            Key::Char('x' | 'X' | 'D') => return Ok(Some(O(Operator::Delete))),
            Key::Char('s' | 'S' | 'C' | 'R') => return Ok(Some(O(Operator::Change))),
            Key::Char('Y') => return Ok(Some(O(Operator::Yank))),
            Key::Char('u') => return Ok(Some(O(Operator::Lower))),
            Key::Char('U') => return Ok(Some(O(Operator::Upper))),
            Key::Char('o' | 'O') => return action(Action::SwapEnds),
            _ => {}
        }
    }
    match first {
        Key::Escape | Key::Ctrl('[' | 'c') => action(Action::Escape),
        Key::Char('i') => action(Action::Insert),
        Key::Char('a') => action(Action::Append),
        Key::Char('I') => action(Action::InsertAtStart),
        Key::Char('A') => action(Action::AppendAtEnd),
        Key::Char('o') => action(Action::OpenBelow),
        Key::Char('O') => action(Action::OpenAbove),
        Key::Char('x') => action(Action::DeleteChar),
        Key::Char('X') => action(Action::DeleteBack),
        Key::Char('s') => action(Action::Substitute),
        Key::Char('S') => action(Action::SubstituteLine),
        Key::Char('C') => action(Action::ChangeToEnd),
        Key::Char('D') => action(Action::DeleteToEnd),
        Key::Char('Y') => action(Action::YankLine),
        Key::Char('p') => action(Action::Put { after: true }),
        Key::Char('P') => action(Action::Put { after: false }),
        Key::Char('u') => action(Action::Undo),
        Key::Ctrl('r') => action(Action::Redo),
        Key::Char('.') => action(Action::Repeat),
        Key::Char('J') => action(Action::Join),
        Key::Char('~') => action(Action::ToggleCase),
        Key::Char('v') => action(Action::Visual),
        Key::Char('V') => action(Action::VisualLine),
        Key::Char(':') => action(Action::CommandLine),
        Key::Char('/') => action(Action::Search { forward: true }),
        Key::Char('?') => action(Action::Search { forward: false }),
        Key::Char('n') => action(Action::SearchNext { reverse: false }),
        Key::Char('N') => action(Action::SearchNext { reverse: true }),
        Key::Char('*') => action(Action::SearchWord { forward: true }),
        Key::Char('#') => action(Action::SearchWord { forward: false }),
        Key::Char('r') => match keys.get(1) {
            None => Ok(None),
            Some(Key::Char(c)) => action(Action::Replace(*c)),
            Some(Key::Enter) => action(Action::Replace('\n')),
            Some(_) => Err(()),
        },
        _ => Err(()),
    }
}

// ---- Text -----------------------------------------------------------------

pub(crate) fn char_at(text: &str, at: usize) -> Option<char> {
    text.get(at..)?.chars().next()
}

pub(crate) fn next_char(text: &str, at: usize) -> usize {
    char_at(text, at).map_or(at, |c| at + c.len_utf8())
}

pub(crate) fn prev_char(text: &str, at: usize) -> usize {
    text.get(..at)
        .and_then(|before| before.chars().next_back())
        .map_or(0, |c| at - c.len_utf8())
}

/// Applies `step` up to `n` times, stopping when it has nowhere to go.
pub(crate) fn repeat(n: usize, from: usize, mut step: impl FnMut(usize) -> Option<usize>) -> usize {
    let mut at = from;
    for _ in 0..n {
        match step(at) {
            Some(next) if next != at => at = next,
            _ => break,
        }
    }
    at
}

pub(crate) fn line_start(text: &str, at: usize) -> usize {
    text[..at.min(text.len())]
        .rfind('\n')
        .map_or(0, |ix| ix + 1)
}

/// Where the line's text ends, before its line ending.
pub(crate) fn line_end(text: &str, at: usize) -> usize {
    let at = at.min(text.len());
    let end = text[at..].find('\n').map_or(text.len(), |ix| at + ix);
    if end > line_start(text, at)
        && text.as_bytes().get(end - 1) == Some(&b'\r')
        && end < text.len()
    {
        end - 1
    } else {
        end
    }
}

/// Where the next line starts, after this one's line ending.
pub(crate) fn line_end_with_newline(text: &str, at: usize) -> usize {
    let at = at.min(text.len());
    text[at..].find('\n').map_or(text.len(), |ix| at + ix + 1)
}

pub(crate) fn line_ending(text: &str) -> &'static str {
    if text.contains("\r\n") { "\r\n" } else { "\n" }
}

pub(crate) fn line_of(text: &str, at: usize) -> usize {
    text[..at.min(text.len())].matches('\n').count()
}

pub(crate) fn line_count(text: &str) -> usize {
    text.matches('\n').count() + 1
}

/// Where line `index` starts; past the last line, the last line.
pub(crate) fn line_offset(text: &str, index: usize) -> usize {
    let mut start = 0;
    for _ in 0..index {
        match text[start..].find('\n') {
            Some(ix) => start += ix + 1,
            None => break,
        }
    }
    start
}

pub(crate) fn first_non_blank(text: &str, line: usize) -> usize {
    let end = line_end(text, line);
    text[line..end]
        .char_indices()
        .find(|(_, c)| !matches!(c, ' ' | '\t'))
        .map_or(end, |(ix, _)| line + ix)
}

/// Where the cursor belongs in normal mode: on a character, never past a
/// line's last one.
pub fn normal_cursor(text: &str, at: usize) -> usize {
    let at = at.min(text.len());
    let start = line_start(text, at);
    let end = line_end(text, at);
    if at >= end && end > start {
        prev_char(text, end)
    } else {
        at.min(end.max(start))
    }
}

pub(crate) fn column(text: &str, at: usize) -> usize {
    text[line_start(text, at)..at].chars().count()
}

pub(crate) fn at_column(text: &str, line: usize, column: usize) -> usize {
    let end = line_end(text, line);
    text[line..end]
        .char_indices()
        .nth(column)
        .map_or(end, |(ix, _)| line + ix)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Class {
    Blank,
    Newline,
    Word,
    Punctuation,
}

pub(crate) fn class(c: char, big: bool) -> Class {
    if c == '\n' {
        Class::Newline
    } else if c.is_whitespace() {
        Class::Blank
    } else if big || c.is_alphanumeric() || c == '_' {
        Class::Word
    } else {
        Class::Punctuation
    }
}

pub(crate) fn class_at(text: &str, at: usize, big: bool) -> Option<Class> {
    char_at(text, at).map(|c| class(c, big))
}

/// `w`: the start of the next word; an empty line counts as one.
pub(crate) fn word_start(text: &str, from: usize, big: bool) -> usize {
    let mut at = from;
    let start = class_at(text, at, big);
    if let Some(class) = start.filter(|c| matches!(c, Class::Word | Class::Punctuation)) {
        while class_at(text, at, big) == Some(class) {
            at = next_char(text, at);
        }
    }
    let mut newlines = usize::from(start == Some(Class::Newline));
    loop {
        match class_at(text, at, big) {
            Some(Class::Blank) => at = next_char(text, at),
            Some(Class::Newline) => {
                newlines += 1;
                at = next_char(text, at);
                if newlines >= 2 || class_at(text, at, big) == Some(Class::Newline) {
                    // An empty line stops `w`.
                    if class_at(text, at, big) == Some(Class::Newline) {
                        return at;
                    }
                }
            }
            _ => return at,
        }
    }
}

/// `e`: the last character of this or the next word.
pub(crate) fn word_end(text: &str, from: usize, big: bool) -> usize {
    let mut at = next_char(text, from);
    while matches!(class_at(text, at, big), Some(Class::Blank | Class::Newline)) {
        at = next_char(text, at);
    }
    let Some(class) = class_at(text, at, big) else {
        return prev_char(text, text.len()).max(from);
    };
    loop {
        let next = next_char(text, at);
        if next == at || class_at(text, next, big) != Some(class) {
            return at;
        }
        at = next;
    }
}

/// `b`: the start of this or the previous word.
pub(crate) fn word_back(text: &str, from: usize, big: bool) -> usize {
    let mut at = prev_char(text, from);
    if at == from {
        return from;
    }
    while at > 0 && matches!(class_at(text, at, big), Some(Class::Blank | Class::Newline)) {
        // An empty line stops `b`.
        if class_at(text, at, big) == Some(Class::Newline)
            && class_at(text, prev_char(text, at), big) == Some(Class::Newline)
        {
            return at;
        }
        at = prev_char(text, at);
    }
    let class = class_at(text, at, big);
    while at > 0 && class_at(text, prev_char(text, at), big) == class {
        at = prev_char(text, at);
    }
    at
}

/// `ge`: the end of the previous word.
pub(crate) fn word_end_back(text: &str, from: usize, big: bool) -> usize {
    let class = class_at(text, from, big);
    let mut at = from;
    if matches!(class, Some(Class::Word | Class::Punctuation)) {
        while at > 0 && class_at(text, at, big) == class {
            at = prev_char(text, at);
        }
    }
    while at > 0 && matches!(class_at(text, at, big), Some(Class::Blank | Class::Newline)) {
        at = prev_char(text, at);
    }
    at
}

pub(crate) fn blank_line(text: &str, line: usize) -> bool {
    text[line..line_end(text, line)].trim().is_empty()
}

/// `}`: the next blank line after this paragraph, or the end.
fn paragraph_forward(text: &str, from: usize) -> usize {
    let mut line = line_start(text, from);
    // Leave the blank lines the cursor is in, then the paragraph.
    while line < text.len() && blank_line(text, line) {
        line = line_end_with_newline(text, line);
    }
    while line < text.len() && !blank_line(text, line) {
        let next = line_end_with_newline(text, line);
        if next == line {
            break;
        }
        line = next;
    }
    if line >= text.len() {
        normal_cursor(text, text.len())
    } else {
        line
    }
}

/// `{`: the blank line before this paragraph, or the start.
fn paragraph_back(text: &str, from: usize) -> usize {
    let mut line = line_start(text, from);
    // Leave the blank lines the cursor is in, then the paragraph.
    while line > 0 && blank_line(text, line) {
        line = line_start(text, line - 1);
    }
    while line > 0 {
        let previous = line_start(text, line - 1);
        if blank_line(text, previous) {
            return previous;
        }
        line = previous;
    }
    0
}

const fn reversed(kind: FindKind) -> FindKind {
    match kind {
        FindKind::Forward => FindKind::Backward,
        FindKind::Backward => FindKind::Forward,
        FindKind::TillForward => FindKind::TillBackward,
        FindKind::TillBackward => FindKind::TillForward,
    }
}

/// `f`, `t`, `F` and `T` on the cursor's line. `again` notes `;`, which
/// steps past a `t` target the cursor is already beside.
fn find_in_line(
    text: &str,
    from: usize,
    kind: FindKind,
    target: char,
    n: usize,
    again: bool,
) -> Option<Landing> {
    let start = line_start(text, from);
    let end = line_end(text, from);
    let line = &text[start..end];
    let forward = matches!(kind, FindKind::Forward | FindKind::TillForward);
    let till = matches!(kind, FindKind::TillForward | FindKind::TillBackward);
    let mut found = None;
    let mut seen = 0;
    if forward {
        let mut skip = next_char(text, from);
        if till && again {
            skip = next_char(text, skip);
        }
        for (ix, c) in line.char_indices().filter(|(ix, _)| start + ix >= skip) {
            if c == target {
                seen += 1;
                if seen == n {
                    found = Some(start + ix);
                    break;
                }
            }
        }
        let at = found?;
        Some(Landing::inclusive(if till {
            prev_char(text, at)
        } else {
            at
        }))
    } else {
        let mut limit = from;
        if till && again {
            limit = prev_char(text, limit);
        }
        for (ix, c) in line
            .char_indices()
            .rev()
            .filter(|(ix, _)| start + ix < limit)
        {
            if c == target {
                seen += 1;
                if seen == n {
                    found = Some(start + ix);
                    break;
                }
            }
        }
        let at = found?;
        Some(Landing::exclusive(if till {
            next_char(text, at)
        } else {
            at
        }))
    }
}

/// `%`: the bracket matching the one at or after the cursor on its line.
pub(crate) fn match_bracket(text: &str, from: usize) -> Option<usize> {
    let end = line_end(text, from);
    let (at, c) = text[from..end]
        .char_indices()
        .find(|(_, c)| "()[]{}".contains(*c))
        .map(|(ix, c)| (from + ix, c))?;
    let (open, close, forward) = match c {
        '(' => ('(', ')', true),
        ')' => ('(', ')', false),
        '[' => ('[', ']', true),
        ']' => ('[', ']', false),
        '{' => ('{', '}', true),
        _ => ('{', '}', false),
    };
    if forward {
        let mut depth = 0usize;
        for (ix, c) in text[at..].char_indices() {
            if c == open {
                depth += 1;
            } else if c == close {
                depth -= 1;
                if depth == 0 {
                    return Some(at + ix);
                }
            }
        }
    } else {
        let mut depth = 0usize;
        for (ix, c) in text[..=at].char_indices().rev() {
            if c == close {
                depth += 1;
            } else if c == open {
                depth -= 1;
                if depth == 0 {
                    return Some(ix);
                }
            }
        }
    }
    None
}

/// The range of a text object around `at`, and whether it is whole lines.
pub(crate) fn object_range(object: Object, text: &str, at: usize) -> Option<(Range<usize>, bool)> {
    match object {
        Object::Word { big, around } => Some((word_object(text, at, big, around), false)),
        Object::Sentence { around } => Some((sentence_object(text, at, around), false)),
        Object::Paragraph { around } => Some((paragraph_object(text, at, around), true)),
        Object::Quote { quote, around } => {
            quote_object(text, at, quote, around).map(|r| (r, false))
        }
        Object::Bracket {
            open,
            close,
            around,
        } => bracket_object(text, at, open, close, around).map(|r| (r, false)),
    }
}

fn word_object(text: &str, at: usize, big: bool, around: bool) -> Range<usize> {
    let start_line = line_start(text, at);
    let end_line = line_end(text, at);
    let Some(class) = class_at(text, at, big).filter(|c| *c != Class::Newline) else {
        return at..at;
    };
    let mut start = at;
    while start > start_line && class_at(text, prev_char(text, start), big) == Some(class) {
        start = prev_char(text, start);
    }
    let mut end = at;
    while end < end_line && class_at(text, end, big) == Some(class) {
        end = next_char(text, end);
    }
    if !around {
        return start..end;
    }
    if class == Class::Blank {
        // Blanks and the word after them.
        let word = class_at(text, end, big);
        while end < end_line && class_at(text, end, big) == word && word != Some(Class::Blank) {
            end = next_char(text, end);
        }
        return start..end;
    }
    let mut trailing = end;
    while trailing < end_line && class_at(text, trailing, big) == Some(Class::Blank) {
        trailing = next_char(text, trailing);
    }
    if trailing > end {
        return start..trailing;
    }
    while start > start_line && class_at(text, prev_char(text, start), big) == Some(Class::Blank) {
        start = prev_char(text, start);
    }
    start..end
}

/// The paragraph around `at`, as lines: the run of non-blank lines, or of
/// blank ones; `ap` adds the blank lines after it.
fn paragraph_object(text: &str, at: usize, around: bool) -> Range<usize> {
    let blank = blank_line(text, line_start(text, at));
    let mut start = line_start(text, at);
    while start > 0 && blank_line(text, line_start(text, start - 1)) == blank {
        start = line_start(text, start - 1);
    }
    let mut end = line_end_with_newline(text, at);
    while end < text.len() && blank_line(text, end) == blank {
        end = line_end_with_newline(text, end);
    }
    if around && !blank {
        let before = end;
        while end < text.len() && blank_line(text, end) {
            end = line_end_with_newline(text, end);
        }
        if end == before {
            while start > 0 && blank_line(text, line_start(text, start - 1)) {
                start = line_start(text, start - 1);
            }
        }
    }
    start..end
}

/// The sentence around `at` in its paragraph: up to and including `.`, `!`
/// or `?`; `as` adds the blanks after it.
fn sentence_object(text: &str, at: usize, around: bool) -> Range<usize> {
    let paragraph = paragraph_object(text, at, false);
    let block = &text[paragraph.clone()];
    let ends_sentence = |ix: usize| {
        let before = block[..ix].chars().next_back();
        before.is_some_and(|c| matches!(c, '.' | '!' | '?'))
            && block[ix..].chars().next().is_none_or(char::is_whitespace)
    };
    let local = at - paragraph.start;
    let mut start = 0;
    for (ix, _) in block.char_indices() {
        if ix > local {
            break;
        }
        if ix > 0 && ends_sentence(ix) {
            start = ix;
        }
    }
    while block[start..].starts_with(char::is_whitespace) && start < local {
        start += block[start..].chars().next().map_or(1, char::len_utf8);
    }
    let mut end = block.trim_end().len();
    for (ix, _) in block
        .char_indices()
        .chain(std::iter::once((block.len(), ' ')))
    {
        if ix > local && ends_sentence(ix) {
            end = ix;
            break;
        }
    }
    if around {
        while end < block.len() && block[end..].starts_with([' ', '\t']) {
            end += 1;
        }
    }
    paragraph.start + start..paragraph.start + end.max(start)
}

fn quote_object(text: &str, at: usize, quote: char, around: bool) -> Option<Range<usize>> {
    let start = line_start(text, at);
    let end = line_end(text, at);
    let quotes: Vec<usize> = text[start..end]
        .char_indices()
        .filter(|&(ix, c)| c == quote && !text[..start + ix].ends_with('\\'))
        .map(|(ix, _)| start + ix)
        .collect();
    let pair = quotes
        .chunks_exact(2)
        .find(|pair| pair[0] <= at && at <= pair[1])
        .or_else(|| quotes.chunks_exact(2).find(|pair| pair[0] > at))?;
    let (open, close) = (pair[0], pair[1]);
    if around {
        let mut end = close + quote.len_utf8();
        while end < line_end(text, at) && text[end..].starts_with([' ', '\t']) {
            end += 1;
        }
        Some(open..end)
    } else {
        Some(open + quote.len_utf8()..close)
    }
}

fn bracket_object(
    text: &str,
    at: usize,
    open: char,
    close: char,
    around: bool,
) -> Option<Range<usize>> {
    let mut depth = 0usize;
    let mut opening = None;
    let from = if char_at(text, at) == Some(close) {
        at
    } else {
        next_char(text, at)
    };
    for (ix, c) in text[..from].char_indices().rev() {
        if c == close && ix != at {
            depth += 1;
        } else if c == open {
            if depth == 0 {
                opening = Some(ix);
                break;
            }
            depth -= 1;
        }
    }
    let opening = opening?;
    let mut depth = 0usize;
    let mut closing = None;
    for (ix, c) in text[opening..].char_indices() {
        if c == open {
            depth += 1;
        } else if c == close {
            depth -= 1;
            if depth == 0 {
                closing = Some(opening + ix);
                break;
            }
        }
    }
    let closing = closing?;
    Some(if around {
        opening..closing + close.len_utf8()
    } else {
        opening + open.len_utf8()..closing
    })
}

/// A linewise range to delete: after the last line, the line ending before
/// it goes instead, so no empty line is left behind.
pub(crate) fn whole_lines(text: &str, range: Range<usize>) -> Range<usize> {
    if range.end == text.len() && !text.ends_with('\n') && range.start > 0 {
        let mut start = range.start - 1;
        if start > 0 && text.as_bytes()[start - 1] == b'\r' {
            start -= 1;
        }
        start..range.end
    } else {
        range
    }
}

pub(crate) fn edited(text: &str, range: &Range<usize>, new: &str) -> String {
    let mut after = text.to_owned();
    after.replace_range(range.clone(), new);
    after
}

pub(crate) fn toggle_case(c: char) -> Vec<char> {
    if c.is_uppercase() {
        c.to_lowercase().collect()
    } else {
        c.to_uppercase().collect()
    }
}

/// `J`: joins `lines` lines from the one at `at`, with one space between
/// them and without the next lines' leading blanks.
pub(crate) fn join(text: &str, at: usize, lines: usize) -> Vec<Command> {
    let start = line_start(text, at);
    let mut end = line_end(text, at);
    let mut joined = text[start..end].to_owned();
    let mut caret = start + joined.len();
    for _ in 1..lines {
        let next = line_end_with_newline(text, end);
        if next >= text.len() && next == end {
            break;
        }
        if next == end {
            break;
        }
        let next_end = line_end(text, next);
        let content = text[next..next_end].trim_start_matches([' ', '\t']);
        let trimmed = joined.trim_end_matches([' ', '\t']).len();
        joined.truncate(trimmed);
        caret = start + joined.len();
        if !content.is_empty() && !joined.is_empty() && !content.starts_with(')') {
            joined.push(' ');
        }
        joined.push_str(content);
        end = next_end;
    }
    if end == line_end(text, at) {
        return Vec::new();
    }
    vec![Command::Edit {
        range: start..end,
        text: joined,
        caret,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stand-in for the editor: applies Vim's commands to a string.
    struct Sim {
        vim: Vim,
        text: String,
        anchor: usize,
        head: usize,
        clipboard: Option<String>,
        undo: Vec<(String, usize)>,
        found: Vec<String>,
    }

    impl Sim {
        /// `text` with `|` marking the caret.
        fn new(marked: &str) -> Self {
            let head = marked.find('|').expect("a caret");
            let text = marked.replacen('|', "", 1);
            Self {
                vim: Vim::new(),
                text,
                anchor: head,
                head,
                clipboard: None,
                undo: Vec::new(),
                found: Vec::new(),
            }
        }

        fn shown(&self) -> String {
            let mut text = self.text.clone();
            text.insert(self.head, '|');
            text
        }

        fn keys(&mut self, keys: &str) -> &mut Self {
            for key in parse_keys(keys) {
                self.key(key);
            }
            self
        }

        fn key(&mut self, key: Key) {
            let clipboard = self.clipboard.clone();
            let cx = Context {
                text: &self.text,
                anchor: self.anchor,
                head: self.head,
                clipboard: clipboard.as_deref(),
            };
            match self.vim.key(key, &cx) {
                Some(commands) => {
                    for command in commands {
                        self.apply(command);
                    }
                    let (anchor, head) = self.vim.settle(&self.text, self.anchor, self.head);
                    self.anchor = anchor;
                    self.head = head;
                }
                None => self.type_key(key),
            }
        }

        fn type_key(&mut self, key: Key) {
            match key {
                Key::Char(c) => {
                    self.text.insert(self.head, c);
                    self.head += c.len_utf8();
                    self.vim.typed(&c.to_string());
                }
                Key::Enter => {
                    self.text.insert(self.head, '\n');
                    self.head += 1;
                    self.vim.typed("\n");
                }
                Key::Backspace if self.head > 0 => {
                    let at = prev_char(&self.text, self.head);
                    self.text.replace_range(at..self.head, "");
                    self.head = at;
                    self.vim.backspaced();
                }
                _ => {}
            }
            self.anchor = self.head;
        }

        fn apply(&mut self, command: Command) {
            match command {
                Command::Select { anchor, head } => {
                    self.anchor = anchor;
                    self.head = head;
                }
                Command::Edit { range, text, caret } => {
                    self.undo.push((self.text.clone(), self.head));
                    self.text.replace_range(range, &text);
                    self.anchor = caret;
                    self.head = caret;
                }
                Command::Editor(EditorCommand::Down | EditorCommand::Up) => {
                    let down = command == Command::Editor(EditorCommand::Down);
                    let column = column(&self.text, self.head);
                    let line = if down {
                        line_end_with_newline(&self.text, self.head)
                    } else if line_start(&self.text, self.head) > 0 {
                        line_start(&self.text, line_start(&self.text, self.head) - 1)
                    } else {
                        0
                    };
                    let line = if line > self.text.len()
                        || (down && line == self.text.len() && !self.text.ends_with('\n'))
                    {
                        line_start(&self.text, self.head)
                    } else {
                        line
                    };
                    self.head = at_column(&self.text, line, column);
                    self.anchor = self.head;
                }
                Command::Editor(EditorCommand::Newline) => {
                    self.text.insert(self.head, '\n');
                    self.head += 1;
                    self.anchor = self.head;
                }
                Command::Editor(EditorCommand::Indent { range, outdent }) => {
                    let mut line = line_start(&self.text, range.start);
                    let mut end = range.end;
                    loop {
                        if outdent {
                            if self.text[line..].starts_with('\t') {
                                self.text.remove(line);
                                end -= 1;
                            }
                        } else {
                            self.text.insert(line, '\t');
                            end += 1;
                        }
                        let next = line_end_with_newline(&self.text, line);
                        if next >= end || next >= self.text.len() {
                            break;
                        }
                        line = next;
                    }
                }
                Command::Undo => {
                    if let Some((text, head)) = self.undo.pop() {
                        self.text = text;
                        self.head = head;
                        self.anchor = head;
                    }
                }
                Command::Copy(text) => self.clipboard = Some(text),
                Command::FindWord { word, .. } => self.found.push(word),
                Command::Repeat { keys, text } => {
                    self.vim.set_replaying(true);
                    for key in keys {
                        self.key(key);
                    }
                    if self.vim.mode() == Mode::Insert {
                        for c in text.chars() {
                            self.type_key(if c == '\n' { Key::Enter } else { Key::Char(c) });
                        }
                        self.key(Key::Escape);
                    }
                    self.vim.set_replaying(false);
                }
                Command::Redo
                | Command::BeginGroup
                | Command::EndGroup
                | Command::Find { .. }
                | Command::FindNext { .. }
                | Command::Save
                | Command::Close => {}
            }
        }
    }

    /// Keys as typed: `<Esc>`, `<CR>`, `<BS>` and `<C-r>` name special keys.
    fn parse_keys(keys: &str) -> Vec<Key> {
        let mut out = Vec::new();
        let mut rest = keys;
        while let Some(c) = rest.chars().next() {
            if c == '<'
                && let Some(end) = rest.find('>')
                && end > 1
            {
                let name = &rest[1..end];
                let key = match name {
                    "Esc" => Some(Key::Escape),
                    "CR" => Some(Key::Enter),
                    "BS" => Some(Key::Backspace),
                    _ => name
                        .strip_prefix("C-")
                        .and_then(|c| c.chars().next())
                        .map(Key::Ctrl),
                };
                if let Some(key) = key {
                    out.push(key);
                    rest = &rest[end + 1..];
                    continue;
                }
            }
            out.push(Key::Char(c));
            rest = &rest[c.len_utf8()..];
        }
        out
    }

    fn check(start: &str, keys: &str, expected: &str) {
        let mut sim = Sim::new(start);
        sim.keys(keys);
        assert_eq!(sim.shown(), expected, "{start:?} after {keys:?}");
    }

    #[test]
    fn hjkl_and_counts_move() {
        check("|one two three", "l", "o|ne two three");
        check("|one two three", "3l", "one| two three");
        check("one tw|o", "10l", "one tw|o");
        check("one tw|o", "h", "one t|wo");
        check("one\nt|wo\nthree", "j", "one\ntwo\nt|hree");
        check("one\nt|wo\nthree", "k", "o|ne\ntwo\nthree");
    }

    #[test]
    fn word_motions() {
        check("|one two, three", "w", "one |two, three");
        check("|one two, three", "2w", "one two|, three");
        check("|one two, three", "W", "one |two, three");
        check("|one two, three", "2W", "one two, |three");
        check("|one two", "e", "on|e two");
        check("one tw|o", "b", "one |two");
        check("one |two", "b", "|one two");
        check("one\n\n|two", "b", "one\n|\ntwo");
        check("one t|wo", "ge", "on|e two");
    }

    #[test]
    fn line_motions() {
        check("  one |two", "0", "|  one two");
        check("  one |two", "^", "  |one two");
        check("|one two", "$", "one tw|o");
        check("a\nb\n|c", "gg", "|a\nb\nc");
        check("|a\nb\nc", "G", "a\nb\n|c");
        check("|a\nb\nc", "2G", "a\n|b\nc");
        check("|a\n  b", "<CR>", "a\n  |b");
        check("a\n\nb\nc\n\n|d", "{", "a\n\nb\nc\n|\nd");
        check("|a\nb\n\nc", "}", "a\nb\n|\nc");
    }

    #[test]
    fn finding_in_the_line() {
        check("|a,b,c", "f,", "a|,b,c");
        check("|a,b,c", "2f,", "a,b|,c");
        check("|a,b,c", "f,;", "a,b|,c");
        check("|a,b,c", "t,", "|a,b,c");
        check("a|,b,c", "t,", "a,|b,c");
        check("a,b,|c", "F,", "a,b|,c");
        check("a,b,|c", "T,", "a,b,|c");
        check("|(a [b] c)", "%", "(a [b] c|)");
    }

    #[test]
    fn deleting() {
        check("|one two", "x", "|ne two");
        check("one tw|o", "x", "one t|w");
        check("|one two", "dw", "|two");
        check("|one two", "d2w", "|");
        check("one |two\nnext", "dw", "one| \nnext");
        check("|one two", "de", "| two");
        check("one |two", "d$", "one| ");
        check("one |two", "D", "one| ");
        check("one t|wo", "db", "one |wo");
        check("a\n|b\nc", "dd", "a\n|c");
        check("a\nb\n|c", "dd", "a\n|b");
        check("|a\nb\nc", "2dd", "|c");
        check("|a\nb\nc", "dj", "|c");
        check("a\nb\n|c", "dk", "|a");
        check("a\n|b\nc", "dG", "|a");
        check("say (hi |there) now", "di(", "say (|) now");
        check("say (hi |there) now", "da(", "say | now");
        check("a \"quo|ted\" b", "di\"", "a \"|\" b");
        check("a \"quo|ted\" b", "da\"", "a |b");
        check("one t|wo three", "diw", "one | three");
        check("one t|wo three", "daw", "one |three");
    }

    #[test]
    fn changing_enters_insert_mode() {
        let mut sim = Sim::new("|one two");
        sim.keys("cwnew");
        assert_eq!(sim.vim.mode(), Mode::Insert);
        sim.keys("<Esc>");
        assert_eq!(sim.shown(), "ne|w two");
        assert_eq!(sim.vim.mode(), Mode::Normal);
        check("  one |two", "ccx<Esc>", "  |x");
        check("one |two", "Cx<Esc>", "one |x");
        check("|one", "sX<Esc>", "|Xne");
        check("a (b |c) d", "ci(x<Esc>", "a (|x) d");
        check(
            "Hello world. This is |it. End.",
            "cisNew.<Esc>",
            "Hello world. New|. End.",
        );
    }

    #[test]
    fn inserting_and_opening_lines() {
        check("one |two", "ix<Esc>", "one |xtwo");
        check("one |two", "ax<Esc>", "one t|xwo");
        check("  one |two", "Ix<Esc>", "  |xone two");
        check("one |two", "Ax<Esc>", "one two|x");
        check("|one\ntwo", "onew<Esc>", "one\nne|w\ntwo");
        check("one\n|two", "Onew<Esc>", "one\nne|w\ntwo");
    }

    #[test]
    fn yanking_and_putting() {
        let mut sim = Sim::new("|one two");
        sim.keys("yw");
        assert_eq!(
            sim.clipboard.as_deref(),
            Some("one "),
            "yanks go to the clipboard"
        );
        sim.keys("$p");
        assert_eq!(sim.shown(), "one twoone| ");
        check("|a\nb", "yyp", "a\n|a\nb");
        check("a\n|b", "yyp", "a\nb\n|b");
        check("a\n|b", "yyP", "a\n|b\nb");
        check("|one two", "dwP", "one| two");
        check("|a\nb", "ddp", "b\n|a");
    }

    #[test]
    fn put_pastes_a_clipboard_copied_elsewhere() {
        let mut sim = Sim::new("|one");
        sim.keys("yw");
        sim.clipboard = Some("X".to_owned());
        sim.keys("P");
        assert_eq!(sim.shown(), "|Xone");
        sim.keys("dwP");
        assert_eq!(
            sim.shown(),
            "Xon|e",
            "Vim's own delete wins once it is newer"
        );
    }

    #[test]
    fn replacing_joining_and_case() {
        check("|abc", "rx", "|xbc");
        check("|abc", "2rx", "x|xc");
        check("|abc", "~", "A|bc");
        check("|abc", "3~", "AB|C");
        check("|one\n  two\nthree", "J", "one| two\nthree");
        check("|one\ntwo\nthree", "3J", "one two| three");
        check("|one two", "gUw", "|ONE two");
        check("|ONE TWO", "guu", "|one two");
    }

    #[test]
    fn visual_mode_selects_and_operates() {
        let mut sim = Sim::new("|one two three");
        sim.keys("vw");
        assert_eq!(sim.vim.mode(), Mode::Visual);
        assert_eq!((sim.anchor, sim.head), (0, 5), "inclusive of the cursor");
        sim.keys("d");
        assert_eq!(sim.shown(), "|wo three");
        assert_eq!(sim.vim.mode(), Mode::Normal);
        check("a\n|b\nc", "Vjd", "|a");
        check("one |two three", "viwy", "one |two three");
        check("one |two three", "viwU", "one |TWO three");
        check("one |two three", "veohd", "one| three");
        check("|ab", "v<Esc>", "|ab");
    }

    #[test]
    fn a_pointer_selection_becomes_visual() {
        let mut sim = Sim::new("|one two");
        sim.anchor = 0;
        sim.head = 3;
        sim.keys("d");
        assert_eq!(sim.shown(), "| two");
    }

    #[test]
    fn undo_and_redo() {
        let mut sim = Sim::new("|one two");
        sim.keys("dw");
        sim.keys("u");
        assert_eq!(sim.text, "one two");
    }

    #[test]
    fn dot_repeats_the_last_change() {
        check("|a b c d", "dw.", "|c d");
        check("|one two three", "cwx<Esc>w.", "x |x three");
        check("|a\nb\nc", "Ax<Esc>j.", "ax\nb|x\nc");
    }

    #[test]
    fn indenting_asks_the_editor() {
        check("|a\nb", ">>", "|\ta\nb");
        check("|a\nb", ">j", "|\ta\n\tb");
        check("|\ta\nb", "<<", "|a\nb");
    }

    #[test]
    fn ex_commands() {
        let mut sim = Sim::new("|a\nb\nc");
        sim.keys(":3<CR>");
        assert_eq!(sim.shown(), "a\nb\n|c");
        let cx = Context {
            text: "a",
            anchor: 0,
            head: 0,
            clipboard: None,
        };
        let mut vim = Vim::new();
        for key in parse_keys(":wq") {
            vim.key(key, &cx);
        }
        assert_eq!(vim.status().as_deref(), Some(":wq"));
        assert_eq!(
            vim.key(Key::Enter, &cx),
            Some(vec![Command::Save, Command::Close])
        );
        for key in parse_keys(":nope<CR>") {
            vim.key(key, &cx);
        }
        assert_eq!(vim.status().as_deref(), Some("Not an editor command: nope"));
    }

    #[test]
    fn searching_asks_the_editor() {
        let mut sim = Sim::new("one |two two");
        sim.keys("*");
        assert_eq!(sim.found, ["two"]);
        let cx = Context {
            text: "a",
            anchor: 0,
            head: 0,
            clipboard: None,
        };
        let mut vim = Vim::new();
        assert_eq!(
            vim.key(Key::Char('?'), &cx),
            Some(vec![Command::Find { forward: false }])
        );
        assert_eq!(
            vim.key(Key::Char('n'), &cx),
            Some(vec![Command::FindNext { forward: false }]),
            "n follows the search's direction"
        );
    }

    #[test]
    fn pending_keys_show_and_escape_clears_them() {
        let cx = Context {
            text: "a",
            anchor: 0,
            head: 0,
            clipboard: None,
        };
        let mut vim = Vim::new();
        vim.key(Key::Char('2'), &cx);
        vim.key(Key::Char('d'), &cx);
        assert_eq!(vim.status().as_deref(), Some("2d"));
        vim.key(Key::Escape, &cx);
        assert_eq!(vim.status(), None);
    }

    #[test]
    fn typing_passes_through_in_insert_mode_only() {
        let cx = Context {
            text: "a",
            anchor: 0,
            head: 0,
            clipboard: None,
        };
        let mut vim = Vim::new();
        assert!(
            vim.key(Key::Char('z'), &cx).is_some(),
            "normal mode keeps keys"
        );
        vim.key(Key::Char('i'), &cx);
        assert_eq!(vim.key(Key::Char('z'), &cx), None);
        assert_eq!(vim.key(Key::Enter, &cx), None);
    }

    #[test]
    fn the_normal_cursor_stays_on_a_character() {
        assert_eq!(normal_cursor("ab\ncd", 2), 1);
        assert_eq!(normal_cursor("ab\n\ncd", 3), 3, "an empty line");
        assert_eq!(normal_cursor("ab", 2), 1);
        assert_eq!(normal_cursor("", 0), 0);
        assert_eq!(normal_cursor("a\r\nb", 1), 0, "CRLF");
    }

    #[test]
    fn crlf_lines_keep_their_endings() {
        check("a\r\n|b\r\nc", "dd", "a\r\n|c");
        check("|a\r\nb", "$", "|a\r\nb");
    }
}
