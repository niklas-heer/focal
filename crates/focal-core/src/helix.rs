//! Helix's selection-first editing as a state machine, beside [`crate::vim`]:
//! motions select, and actions such as `d` work on the selection. It speaks
//! the same keys and commands as Vim mode, so the editor drives both alike.
//!
//! The selection runs from an anchor to a head, both on characters and both
//! included; with the two equal it is the cursor alone, drawn as a block.

use std::ops::Range;

use crate::vim::{
    Command, Context, EditorCommand, HALF_PAGE, Key, Register, at_column, caret, class_at, column,
    first_non_blank, join, key_name, line_end, line_end_with_newline, line_ending, line_of,
    line_offset, line_start, match_bracket, next_char, object_range, prev_char, read_object,
    toggle_case, word_back, word_end, word_start,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Insert,
    /// `v`: motions extend the selection instead of replacing it.
    Select,
}

impl Mode {
    pub const fn label(self) -> Option<&'static str> {
        match self {
            Self::Normal => None,
            Self::Insert => Some("INSERT"),
            Self::Select => Some("SELECT"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Find {
    To,
    Till,
    BackTo,
    BackTill,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Motion {
    Left,
    Right,
    Down,
    Up,
    ScreenDown,
    ScreenUp,
    NextWordStart(bool),
    NextWordEnd(bool),
    PreviousWordStart(bool),
    Find(Find, char),
    FileStart,
    FileEnd,
    LineStart,
    LineEnd,
    FirstNonBlank,
    MatchBracket,
    HalfPageDown,
    HalfPageUp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Action {
    Move(Motion),
    SelectLine,
    ExtendToLineBounds,
    SelectAll,
    Collapse,
    SelectObject(crate::vim::Object),
    SelectMode,
    Escape,
    Delete,
    Change,
    Yank,
    Paste { after: bool },
    ReplaceWithYank,
    Replace(char),
    ToggleCase,
    Lower,
    Indent,
    Outdent,
    Join,
    Undo,
    Redo,
    Insert,
    Append,
    InsertAtStart,
    AppendAtEnd,
    OpenBelow,
    OpenAbove,
    Search { forward: bool },
    SearchNext { reverse: bool },
    SearchSelection,
    CommandLine,
    Repeat,
}

enum Parse {
    Pending,
    Invalid,
    Done {
        count: Option<usize>,
        action: Action,
    },
}

#[derive(Clone, Debug, Default)]
struct Change {
    keys: Vec<Key>,
    text: String,
}

#[derive(Debug)]
pub struct Helix {
    mode: Mode,
    keys: Vec<Key>,
    /// The selection's anchor and head, both included.
    selection: (usize, usize),
    /// The editor selection Helix last set, to notice the pointer.
    shown: Option<(usize, usize)>,
    register: Register,
    clipboard_then: Option<String>,
    search_backward: bool,
    last_change: Option<Change>,
    recording: Option<Change>,
    replaying: bool,
    goal: Option<usize>,
    command_line: Option<String>,
    message: Option<String>,
    grouped: bool,
}

impl Default for Helix {
    fn default() -> Self {
        Self::new()
    }
}

impl Helix {
    pub fn new() -> Self {
        Self {
            mode: Mode::Normal,
            keys: Vec::new(),
            selection: (0, 0),
            shown: None,
            register: Register::default(),
            clipboard_then: None,
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

    pub fn status(&self) -> Option<String> {
        if let Some(line) = &self.command_line {
            return Some(format!(":{line}"));
        }
        if let Some(message) = &self.message {
            return Some(message.clone());
        }
        (!self.keys.is_empty()).then(|| self.keys.iter().map(|key| key_name(*key)).collect())
    }

    /// The block cursor is the selection's head, except while inserting.
    pub const fn block(&self, head: usize) -> Option<usize> {
        match self.mode {
            Mode::Insert => None,
            Mode::Normal | Mode::Select => {
                if self.shown.is_some() {
                    Some(self.selection.1)
                } else {
                    Some(head)
                }
            }
        }
    }

    pub const fn set_replaying(&mut self, replaying: bool) {
        self.replaying = replaying;
    }

    pub fn typed(&mut self, text: &str) {
        if let Some(change) = &mut self.recording {
            change.text.push_str(text);
        }
    }

    pub fn backspaced(&mut self) {
        if let Some(change) = &mut self.recording {
            change.text.pop();
        }
    }

    /// Forgets the selection, as when the pointer starts a new one.
    pub fn reset(&mut self) {
        if self.mode == Mode::Select {
            self.mode = Mode::Normal;
        }
        self.keys.clear();
        self.shown = None;
    }

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
            && !matches!(c, 'd' | 'u' | '[' | 'c')
        {
            return None;
        }
        if self.shown != Some((cx.anchor, cx.head)) {
            self.resync(cx);
        }
        self.keys.push(key);
        match parse(&self.keys) {
            Parse::Pending => Some(Vec::new()),
            Parse::Invalid => {
                self.keys.clear();
                Some(Vec::new())
            }
            Parse::Done { count, action } => {
                let keys = std::mem::take(&mut self.keys);
                if changes_text(action) && !self.replaying {
                    self.recording = Some(Change {
                        keys,
                        text: String::new(),
                    });
                }
                if !matches!(action, Action::Move(Motion::Down | Motion::Up)) {
                    self.goal = None;
                }
                let commands = self.run(action, count.unwrap_or(1).max(1), cx);
                if self.mode != Mode::Insert && self.recording.is_some() {
                    self.last_change = self.recording.take();
                }
                Some(commands)
            }
        }
    }

    /// The selection again from the editor's, after the pointer or the
    /// editor's own moves changed it.
    fn resync(&mut self, cx: &Context) {
        self.selection = match cx.head.cmp(&cx.anchor) {
            std::cmp::Ordering::Equal => (cx.head, cx.head),
            std::cmp::Ordering::Greater => (cx.anchor, prev_char(cx.text, cx.head).max(cx.anchor)),
            std::cmp::Ordering::Less => (prev_char(cx.text, cx.anchor).max(cx.head), cx.head),
        };
        self.shown = Some((cx.anchor, cx.head));
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
        let at = if cx.head > line_start(cx.text, cx.head) {
            prev_char(cx.text, cx.head)
        } else {
            cx.head
        };
        self.selection = (at, at);
        commands.push(self.show(cx.text));
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
                return match line.trim() {
                    "w" | "write" | "w!" => vec![Command::Save],
                    "q" | "quit" | "q!" | "quit!" => vec![Command::Close],
                    "wq" | "x" | "write-quit" | "wq!" => vec![Command::Save, Command::Close],
                    "" => Vec::new(),
                    other => {
                        if let Ok(number) = other.parse::<usize>() {
                            let at = line_offset(cx.text, number.saturating_sub(1));
                            self.selection = (at, at);
                            return vec![self.show(cx.text)];
                        }
                        self.message = Some(format!("no such command: '{other}'"));
                        Vec::new()
                    }
                };
            }
            Key::Escape | Key::Ctrl('[' | 'c') => self.command_line = None,
            _ => {}
        }
        Vec::new()
    }

    /// The selection as the editor shows it: the characters from anchor to
    /// head, or a caret when they are one.
    fn show(&mut self, text: &str) -> Command {
        let (anchor, head) = self.selection;
        let shown = match head.cmp(&anchor) {
            std::cmp::Ordering::Equal => (head, head),
            std::cmp::Ordering::Greater => (anchor, next_char(text, head)),
            std::cmp::Ordering::Less => (next_char(text, anchor), head),
        };
        self.shown = Some(shown);
        Command::Select {
            anchor: shown.0,
            head: shown.1,
        }
    }

    fn range(&self, text: &str) -> Range<usize> {
        let (a, h) = self.selection;
        a.min(h)..next_char(text, a.max(h))
    }

    /// Moves the head; in normal mode the selection starts again at
    /// `anchor`, in select mode it extends.
    fn select(&mut self, anchor: usize, head: usize) {
        if self.mode == Mode::Select {
            self.selection.1 = head;
        } else {
            self.selection = (anchor, head);
        }
    }

    fn write_register(&mut self, text: String, cx: &Context) {
        self.clipboard_then = cx.clipboard.map(str::to_owned);
        let linewise = text.ends_with('\n');
        self.register = Register { text, linewise };
    }

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

    /// Replaces `range` with `new` and selects what replaced it.
    fn replace_selecting(&mut self, text: &str, range: Range<usize>, new: String) -> Vec<Command> {
        let start = range.start;
        let after = crate::vim::edited(text, &range, &new);
        let last = if new.is_empty() {
            start
        } else {
            start + prev_char(&new, new.len())
        };
        let edit = Command::Edit {
            range,
            text: new,
            caret: start,
        };
        self.selection = (start, last);
        vec![edit, self.show(&after)]
    }

    /// Replaces `range` and leaves the cursor at `at`.
    fn edit(&mut self, range: Range<usize>, text: String, at: usize) -> Command {
        self.selection = (at, at);
        self.shown = Some((at, at));
        Command::Edit {
            range,
            text,
            caret: at,
        }
    }

    #[allow(clippy::too_many_lines)]
    fn run(&mut self, action: Action, n: usize, cx: &Context) -> Vec<Command> {
        let text = cx.text;
        let (_, head) = self.selection;
        let mut commands = Vec::new();
        match action {
            Action::Move(motion) => return self.motion(motion, n, cx),
            Action::SelectLine => {
                let range = self.range(text);
                let whole = range.start == line_start(text, range.start)
                    && range.end == line_end_with_newline(text, prev_char(text, range.end));
                let start = line_start(text, range.start);
                let mut end = if whole && !range.is_empty() {
                    line_end_with_newline(text, range.end)
                } else {
                    line_end_with_newline(text, prev_char(text, range.end).max(range.start))
                };
                for _ in 1..n {
                    end = line_end_with_newline(text, end);
                }
                self.selection = (start, prev_char(text, end).max(start));
            }
            Action::ExtendToLineBounds => {
                let range = self.range(text);
                let start = line_start(text, range.start);
                let end = line_end_with_newline(text, prev_char(text, range.end).max(range.start));
                self.selection = (start, prev_char(text, end).max(start));
            }
            Action::SelectAll => self.selection = (0, prev_char(text, text.len())),
            Action::Collapse => self.selection = (head, head),
            Action::SelectObject(object) => {
                if let Some((range, _)) = object_range(object, text, head)
                    && !range.is_empty()
                {
                    self.selection = (range.start, prev_char(text, range.end));
                }
            }
            Action::SelectMode => {
                self.mode = if self.mode == Mode::Select {
                    Mode::Normal
                } else {
                    Mode::Select
                };
            }
            Action::Escape => {
                if self.mode == Mode::Select {
                    self.mode = Mode::Normal;
                } else {
                    self.selection = (head, head);
                }
            }
            Action::Delete | Action::Change => {
                let range = self.range(text);
                if range.is_empty() {
                    return Vec::new();
                }
                self.write_register(text[range.clone()].to_owned(), cx);
                if action == Action::Change {
                    self.begin_change(&mut commands);
                    self.mode = Mode::Insert;
                } else {
                    self.mode = Mode::Normal;
                }
                let at = range.start;
                commands.push(self.edit(range, String::new(), at));
                return commands;
            }
            Action::Yank => {
                let range = self.range(text);
                let yanked = text[range].to_owned();
                self.write_register(yanked.clone(), cx);
                self.clipboard_then = Some(yanked.clone());
                commands.push(Command::Copy(yanked));
                self.mode = Mode::Normal;
            }
            Action::Paste { after } => {
                let register = self.paste_register(cx);
                if register.text.is_empty() {
                    return Vec::new();
                }
                let range = self.range(text);
                let at = match (register.linewise, after) {
                    (true, true) => {
                        line_end_with_newline(text, prev_char(text, range.end).max(range.start))
                    }
                    (true, false) => line_start(text, range.start),
                    (false, true) => range.end,
                    (false, false) => range.start,
                };
                return self.replace_selecting(text, at..at, register.text.repeat(n));
            }
            Action::ReplaceWithYank => {
                let register = self.paste_register(cx);
                return self.replace_selecting(text, self.range(text), register.text);
            }
            Action::Replace(c) => {
                let range = self.range(text);
                let replaced: String = text[range.clone()]
                    .chars()
                    .map(|old| if old == '\n' || old == '\r' { old } else { c })
                    .collect();
                return self.replace_selecting(text, range, replaced);
            }
            Action::ToggleCase | Action::Lower => {
                let range = self.range(text);
                let changed: String = text[range.clone()]
                    .chars()
                    .flat_map(|c| -> Vec<char> {
                        if action == Action::Lower {
                            c.to_lowercase().collect()
                        } else {
                            toggle_case(c)
                        }
                    })
                    .collect();
                return self.replace_selecting(text, range, changed);
            }
            Action::Indent | Action::Outdent => {
                let range = self.range(text);
                commands.push(Command::Editor(EditorCommand::Indent {
                    range,
                    outdent: action == Action::Outdent,
                }));
                // The editor puts the caret on the first line.
                self.shown = None;
                return commands;
            }
            Action::Join => {
                let range = self.range(text);
                let lines = line_of(text, prev_char(text, range.end).max(range.start))
                    - line_of(text, range.start)
                    + 1;
                commands.extend(join(text, range.start, lines.max(2) + n - 1));
                self.shown = None;
                return commands;
            }
            Action::Undo | Action::Redo => {
                let command = if action == Action::Undo {
                    Command::Undo
                } else {
                    Command::Redo
                };
                commands.extend(std::iter::repeat_n(command, n));
                self.shown = None;
                return commands;
            }
            Action::Insert | Action::Append | Action::InsertAtStart | Action::AppendAtEnd => {
                let range = self.range(text);
                let at = match action {
                    Action::Insert => range.start,
                    Action::Append => range.end,
                    Action::InsertAtStart => first_non_blank(text, line_start(text, range.start)),
                    _ => line_end(text, prev_char(text, range.end).max(range.start)),
                };
                self.begin_change(&mut commands);
                commands.push(caret(at));
                self.shown = Some((at, at));
                self.selection = (at, at);
                self.mode = Mode::Insert;
                return commands;
            }
            Action::OpenBelow => {
                let range = self.range(text);
                self.begin_change(&mut commands);
                commands.push(caret(line_end(
                    text,
                    prev_char(text, range.end).max(range.start),
                )));
                commands.push(Command::Editor(EditorCommand::Newline));
                self.mode = Mode::Insert;
                self.shown = None;
                return commands;
            }
            Action::OpenAbove => {
                let start = line_start(text, self.range(text).start);
                self.begin_change(&mut commands);
                self.mode = Mode::Insert;
                commands.push(self.edit(start..start, line_ending(text).to_owned(), start));
                return commands;
            }
            Action::Search { forward } => {
                self.search_backward = !forward;
                return vec![Command::Find { forward }];
            }
            Action::SearchNext { reverse } => {
                self.shown = None;
                return vec![Command::FindNext {
                    forward: self.search_backward == reverse,
                }];
            }
            Action::SearchSelection => {
                let range = self.range(text);
                let word = text[range].to_owned();
                if word.trim().is_empty() {
                    return Vec::new();
                }
                self.search_backward = false;
                self.shown = None;
                return vec![Command::FindWord {
                    word,
                    forward: true,
                }];
            }
            Action::CommandLine => {
                self.command_line = Some(String::new());
                return Vec::new();
            }
            Action::Repeat => {
                if let Some(change) = &self.last_change {
                    return vec![Command::Repeat {
                        keys: change.keys.clone(),
                        text: change.text.clone(),
                    }];
                }
                return Vec::new();
            }
        }
        commands.push(self.show(text));
        commands
    }

    fn motion(&mut self, motion: Motion, n: usize, cx: &Context) -> Vec<Command> {
        let text = cx.text;
        let (_, head) = self.selection;
        // Plain `j` and `k` move by screen line, through the editor.
        let screen = match motion {
            Motion::Down | Motion::ScreenDown if self.mode == Mode::Normal => {
                Some((EditorCommand::Down, n))
            }
            Motion::Up | Motion::ScreenUp if self.mode == Mode::Normal => {
                Some((EditorCommand::Up, n))
            }
            Motion::HalfPageDown => Some((EditorCommand::Down, HALF_PAGE)),
            Motion::HalfPageUp => Some((EditorCommand::Up, HALF_PAGE)),
            _ => None,
        };
        if let Some((command, times)) = screen {
            self.mode = Mode::Normal;
            self.shown = None;
            let mut commands = vec![caret(head)];
            commands.extend(std::iter::repeat_n(Command::Editor(command), times));
            return commands;
        }
        match motion {
            Motion::Left => {
                let at = repeat(n, head, |at| (at > 0).then(|| prev_char(text, at)));
                self.select(at, at);
            }
            Motion::Right => {
                let at = repeat(n, head, |at| (at < text.len()).then(|| next_char(text, at)));
                let at = at.min(prev_char(text, text.len()));
                self.select(at, at);
            }
            Motion::Down | Motion::Up | Motion::ScreenDown | Motion::ScreenUp => {
                let goal = *self.goal.get_or_insert_with(|| column(text, head));
                let line = line_of(text, head);
                let target = if matches!(motion, Motion::Down | Motion::ScreenDown) {
                    line + n
                } else {
                    line.saturating_sub(n)
                };
                let at = at_column(text, line_offset(text, target), goal);
                self.select(at, at);
            }
            Motion::NextWordStart(big) | Motion::NextWordEnd(big) => {
                let end = matches!(motion, Motion::NextWordEnd(_));
                let (anchor, at) = next_word(text, head, n, big, end);
                self.select(anchor, at);
            }
            Motion::PreviousWordStart(big) => {
                let (anchor, at) = previous_word(text, head, n, big);
                self.select(anchor, at);
            }
            Motion::Find(kind, target) => {
                if let Some(at) = find(text, head, kind, target, n) {
                    self.select(head, at);
                }
            }
            Motion::FileStart => self.select(0, 0),
            Motion::FileEnd => {
                let at = line_offset(text, usize::MAX);
                self.select(at, at);
            }
            Motion::LineStart => {
                let at = line_start(text, head);
                self.select(at, at);
            }
            Motion::LineEnd => {
                let end = line_end(text, head);
                let at = if end > line_start(text, head) {
                    prev_char(text, end)
                } else {
                    end
                };
                self.select(at, at);
            }
            Motion::FirstNonBlank => {
                let at = first_non_blank(text, line_start(text, head));
                self.select(at, at);
            }
            Motion::MatchBracket => {
                if let Some(at) = match_bracket(text, head) {
                    self.select(at, at);
                }
            }
            Motion::HalfPageDown | Motion::HalfPageUp => {}
        }
        vec![self.show(text)]
    }
}

/// `w` and `e`: the next word, from its start (or the blanks before it)
/// to its end; at a word's last character, the word after it.
fn next_word(text: &str, head: usize, n: usize, big: bool, end: bool) -> (usize, usize) {
    let mut anchor = head;
    let mut at = head;
    for _ in 0..n {
        let next = next_char(text, at);
        let start = if class_at(text, next, big) == class_at(text, at, big) {
            at
        } else {
            next
        };
        if start >= text.len() {
            break;
        }
        anchor = start;
        at = if end {
            word_end(text, start, big)
        } else {
            prev_char(text, word_start(text, start, big)).max(start)
        };
    }
    (anchor, at)
}

/// `b`: back to the start of this or the previous word.
fn previous_word(text: &str, head: usize, n: usize, big: bool) -> (usize, usize) {
    let mut anchor = head;
    let mut at = head;
    for _ in 0..n {
        let start = if at > 0 && class_at(text, prev_char(text, at), big) != class_at(text, at, big)
        {
            prev_char(text, at)
        } else {
            at
        };
        anchor = start;
        at = word_back(text, next_char(text, start), big);
    }
    (anchor, at)
}

fn repeat(n: usize, from: usize, mut step: impl FnMut(usize) -> Option<usize>) -> usize {
    let mut at = from;
    for _ in 0..n {
        match step(at) {
            Some(next) if next != at => at = next,
            _ => break,
        }
    }
    at
}

/// `f`, `t`, `F` and `T`: the character on the line to select up to.
fn find(text: &str, from: usize, kind: Find, target: char, n: usize) -> Option<usize> {
    let start = line_start(text, from);
    let end = line_end(text, from);
    let forward = matches!(kind, Find::To | Find::Till);
    let mut seen = 0;
    if forward {
        let skip = next_char(text, from);
        for (ix, c) in text[start..end].char_indices() {
            if start + ix >= skip && c == target {
                seen += 1;
                if seen == n {
                    let at = start + ix;
                    return Some(if kind == Find::Till {
                        prev_char(text, at)
                    } else {
                        at
                    });
                }
            }
        }
    } else {
        let limit = from;
        for (ix, c) in text[start..end].char_indices().rev() {
            if start + ix < limit && c == target {
                seen += 1;
                if seen == n {
                    let at = start + ix;
                    return Some(if kind == Find::BackTill {
                        next_char(text, at)
                    } else {
                        at
                    });
                }
            }
        }
    }
    None
}

/// The motions, and the keys starting with `m` or `r`; `None` for other keys.
fn parse_motion(rest: &[Key], count: Option<usize>) -> Option<Parse> {
    let first = *rest.first()?;
    let done = |action| Parse::Done { count, action };
    let second = rest.get(1).copied();
    let motion = |motion| done(Action::Move(motion));
    Some(match first {
        Key::Char('h') | Key::Backspace => motion(Motion::Left),
        Key::Char('l' | ' ') => motion(Motion::Right),
        Key::Char('j') => motion(Motion::Down),
        Key::Char('k') => motion(Motion::Up),
        Key::Char('w') => motion(Motion::NextWordStart(false)),
        Key::Char('W') => motion(Motion::NextWordStart(true)),
        Key::Char('e') => motion(Motion::NextWordEnd(false)),
        Key::Char('E') => motion(Motion::NextWordEnd(true)),
        Key::Char('b') => motion(Motion::PreviousWordStart(false)),
        Key::Char('B') => motion(Motion::PreviousWordStart(true)),
        Key::Ctrl('d') => motion(Motion::HalfPageDown),
        Key::Ctrl('u') => motion(Motion::HalfPageUp),
        Key::Char(c @ ('f' | 't' | 'F' | 'T')) => match second {
            None => Parse::Pending,
            Some(Key::Char(target)) => {
                let kind = match c {
                    'f' => Find::To,
                    't' => Find::Till,
                    'F' => Find::BackTo,
                    _ => Find::BackTill,
                };
                motion(Motion::Find(kind, target))
            }
            Some(_) => Parse::Invalid,
        },
        Key::Char('g') => match second {
            None => Parse::Pending,
            Some(Key::Char('g')) => motion(Motion::FileStart),
            Some(Key::Char('e')) => motion(Motion::FileEnd),
            Some(Key::Char('h')) => motion(Motion::LineStart),
            Some(Key::Char('l')) => motion(Motion::LineEnd),
            Some(Key::Char('s')) => motion(Motion::FirstNonBlank),
            Some(Key::Char('j')) => motion(Motion::ScreenDown),
            Some(Key::Char('k')) => motion(Motion::ScreenUp),
            Some(_) => Parse::Invalid,
        },
        Key::Char('m') => match second {
            None => Parse::Pending,
            Some(Key::Char('m')) => motion(Motion::MatchBracket),
            Some(Key::Char(around @ ('i' | 'a'))) => match rest.get(2) {
                None => Parse::Pending,
                Some(&key) => read_object(key, around == 'a')
                    .map_or(Parse::Invalid, |object| done(Action::SelectObject(object))),
            },
            Some(_) => Parse::Invalid,
        },
        Key::Char('r') => match second {
            None => Parse::Pending,
            Some(Key::Char(c)) => done(Action::Replace(c)),
            Some(Key::Enter) => done(Action::Replace('\n')),
            Some(_) => Parse::Invalid,
        },
        _ => return None,
    })
}

const fn changes_text(action: Action) -> bool {
    matches!(
        action,
        Action::Delete
            | Action::Change
            | Action::Paste { .. }
            | Action::ReplaceWithYank
            | Action::Replace(_)
            | Action::ToggleCase
            | Action::Lower
            | Action::Indent
            | Action::Outdent
            | Action::Join
            | Action::Insert
            | Action::Append
            | Action::InsertAtStart
            | Action::AppendAtEnd
            | Action::OpenBelow
            | Action::OpenAbove
    )
}

fn parse(keys: &[Key]) -> Parse {
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
    let rest = &keys[used..];
    let Some(&first) = rest.first() else {
        return Parse::Pending;
    };
    let done = |action| Parse::Done { count, action };
    if let Some(parse) = parse_motion(rest, count) {
        return parse;
    }
    match first {
        Key::Char('x') => done(Action::SelectLine),
        Key::Char('X') => done(Action::ExtendToLineBounds),
        Key::Char('%') => done(Action::SelectAll),
        Key::Char(';') => done(Action::Collapse),
        Key::Char('v') => done(Action::SelectMode),
        Key::Escape | Key::Ctrl('[' | 'c') => done(Action::Escape),
        Key::Char('d') => done(Action::Delete),
        Key::Char('c') => done(Action::Change),
        Key::Char('y') => done(Action::Yank),
        Key::Char('p') => done(Action::Paste { after: true }),
        Key::Char('P') => done(Action::Paste { after: false }),
        Key::Char('R') => done(Action::ReplaceWithYank),
        Key::Char('~') => done(Action::ToggleCase),
        Key::Char('`') => done(Action::Lower),
        Key::Char('>') => done(Action::Indent),
        Key::Char('<') => done(Action::Outdent),
        Key::Char('J') => done(Action::Join),
        Key::Char('u') => done(Action::Undo),
        Key::Char('U') => done(Action::Redo),
        Key::Char('i') => done(Action::Insert),
        Key::Char('a') => done(Action::Append),
        Key::Char('I') => done(Action::InsertAtStart),
        Key::Char('A') => done(Action::AppendAtEnd),
        Key::Char('o') => done(Action::OpenBelow),
        Key::Char('O') => done(Action::OpenAbove),
        Key::Char('/') => done(Action::Search { forward: true }),
        Key::Char('?') => done(Action::Search { forward: false }),
        Key::Char('n') => done(Action::SearchNext { reverse: false }),
        Key::Char('N') => done(Action::SearchNext { reverse: true }),
        Key::Char('*') => done(Action::SearchSelection),
        Key::Char(':') => done(Action::CommandLine),
        Key::Char('.') => done(Action::Repeat),
        _ => Parse::Invalid,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stand-in editor: the selection runs from `anchor` to `head`,
    /// exclusive like the editor's.
    struct Sim {
        helix: Helix,
        text: String,
        anchor: usize,
        head: usize,
        clipboard: Option<String>,
        undo: Vec<String>,
    }

    impl Sim {
        fn new(marked: &str) -> Self {
            let head = marked.find('|').expect("a caret");
            Self {
                helix: Helix::new(),
                text: marked.replacen('|', "", 1),
                anchor: head,
                head,
                clipboard: None,
                undo: Vec::new(),
            }
        }

        /// The text with the selection in brackets, or `|` for a caret.
        fn shown(&self) -> String {
            let mut text = self.text.clone();
            if self.anchor == self.head {
                text.insert(self.head, '|');
            } else {
                let (lo, hi) = (self.anchor.min(self.head), self.anchor.max(self.head));
                text.insert(hi, ']');
                text.insert(lo, '[');
            }
            text
        }

        fn keys(&mut self, keys: &str) -> &mut Self {
            let mut chars = keys.chars().peekable();
            while let Some(c) = chars.next() {
                let key = if c == '<' {
                    let name: String = chars.by_ref().take_while(|&c| c != '>').collect();
                    match name.as_str() {
                        "Esc" => Key::Escape,
                        "CR" => Key::Enter,
                        _ => Key::Char('<'),
                    }
                } else {
                    Key::Char(c)
                };
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
            match self.helix.key(key, &cx) {
                Some(commands) => {
                    for command in commands {
                        self.apply(command);
                    }
                }
                None => {
                    if let Key::Char(c) = key {
                        self.text.insert(self.head, c);
                        self.head += c.len_utf8();
                        self.anchor = self.head;
                        self.helix.typed(&c.to_string());
                    }
                }
            }
        }

        fn apply(&mut self, command: Command) {
            match command {
                Command::Select { anchor, head } => {
                    self.anchor = anchor;
                    self.head = head;
                }
                Command::Edit { range, text, caret } => {
                    self.undo.push(self.text.clone());
                    self.text.replace_range(range, &text);
                    self.anchor = caret;
                    self.head = caret;
                }
                Command::Copy(text) => self.clipboard = Some(text),
                Command::Undo => {
                    if let Some(text) = self.undo.pop() {
                        self.text = text;
                        self.head = self.head.min(self.text.len());
                        self.anchor = self.head;
                    }
                }
                Command::Editor(EditorCommand::Newline) => {
                    self.text.insert(self.head, '\n');
                    self.head += 1;
                    self.anchor = self.head;
                }
                Command::Repeat { keys, text } => {
                    self.helix.set_replaying(true);
                    for key in keys {
                        self.key(key);
                    }
                    if self.helix.mode() == Mode::Insert {
                        for c in text.chars() {
                            self.key(Key::Char(c));
                        }
                        self.key(Key::Escape);
                    }
                    self.helix.set_replaying(false);
                }
                _ => {}
            }
        }
    }

    fn check(start: &str, keys: &str, expected: &str) {
        let mut sim = Sim::new(start);
        sim.keys(keys);
        assert_eq!(sim.shown(), expected, "{start:?} after {keys:?}");
    }

    #[test]
    fn word_motions_select_words() {
        check("|one two three", "w", "[one ]two three");
        check("|one two three", "ww", "one [two ]three");
        check("|one two three", "e", "[one] two three");
        check("|one two three", "ee", "one[ two] three");
        check("one two thre|e", "b", "one two [three]");
        check("|one two three", "2w", "one [two ]three");
    }

    #[test]
    fn select_mode_extends() {
        check("|one two three", "vww", "[one two ]three");
        check("|one two three", "vw<Esc>", "[one ]two three");
    }

    #[test]
    fn actions_work_on_the_selection() {
        check("|one two three", "wd", "|two three");
        check("|one two three", "ecx<Esc>", "|x two three");
        check("one |two three", "miwd", "one | three");
        check("say (|hi) now", "mi(d", "say (|) now");
        check("|one two", "w~", "[ONE ]two");
        check("|abc", "%rx", "[xxx]");
    }

    #[test]
    fn x_selects_whole_lines() {
        check("|a\nb\nc", "x", "[a\n]b\nc");
        check("|a\nb\nc", "xx", "[a\nb\n]c");
        check("|a\nb\nc", "xd", "|b\nc");
    }

    #[test]
    fn yank_and_paste() {
        let mut sim = Sim::new("|one two");
        sim.keys("wy");
        assert_eq!(sim.clipboard.as_deref(), Some("one "));
        sim.keys(";glp");
        assert_eq!(sim.text, "one twoone ");
        check("|a\nb", "xyp", "a\n[a\n]b");
    }

    #[test]
    fn going_places() {
        check("a\nb\n|c", "gg", "|a\nb\nc");
        check("|a\nb\nc", "ge", "a\nb\n|c");
        check("  one |two", "gh", "|  one two");
        check("  one |two", "gs", "  |one two");
        check("|one two", "gl", "one tw|o");
        check("|a,b,c", "f,", "[a,]b,c");
        check("|abc,d", "t,", "[abc],d");
    }

    #[test]
    fn inserting_appending_and_repeating() {
        check("|one two", "ix<Esc>", "|xone two");
        check("|one two", "eax<Esc>", "one|x two");
        check("|one two", "Ax<Esc>", "one two|x");
        let mut sim = Sim::new("|one two three");
        sim.keys("wd");
        sim.keys("w.");
        assert_eq!(sim.text, "three");
    }

    #[test]
    fn typing_passes_through_only_while_inserting() {
        let cx = Context {
            text: "a",
            anchor: 0,
            head: 0,
            clipboard: None,
        };
        let mut helix = Helix::new();
        assert!(helix.key(Key::Char('z'), &cx).is_some());
        helix.key(Key::Char('i'), &cx);
        assert_eq!(helix.key(Key::Char('z'), &cx), None);
    }

    #[test]
    fn commands_and_status() {
        let cx = Context {
            text: "a\nb",
            anchor: 0,
            head: 0,
            clipboard: None,
        };
        let mut helix = Helix::new();
        helix.key(Key::Char('g'), &cx);
        assert_eq!(helix.status().as_deref(), Some("g"));
        helix.key(Key::Escape, &cx);
        for c in ":wq".chars() {
            helix.key(Key::Char(c), &cx);
        }
        assert_eq!(
            helix.key(Key::Enter, &cx),
            Some(vec![Command::Save, Command::Close])
        );
    }
}
