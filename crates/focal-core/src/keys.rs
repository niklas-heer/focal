//! What Vim's and Helix's keys do, in words: the hints shown while a command
//! waits for its next key, the space menu of Focal's own commands, the `:`
//! commands, and the full reference. The engines in [`crate::vim`] and
//! [`crate::helix`] read their menus and commands from here, so the help can
//! never drift from what the keys do.

use crate::vim::{Command, caret, first_non_blank, line_offset};

/// Keys, and what they do.
pub type Hint = (&'static str, &'static str);

/// The hints for a waiting command: what it is, and the keys that finish it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hints {
    pub title: &'static str,
    pub hints: Vec<Hint>,
}

impl Hints {
    fn new(title: &'static str, hints: &[Hint]) -> Self {
        Self {
            title,
            hints: hints.to_vec(),
        }
    }
}

/// Focal's own commands, which the space menu and `:` reach.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppCommand {
    OpenFile,
    GoToHeading,
    Find,
    Info,
    Outline,
    Sidebar,
    Recent,
    DarkMode,
    FocusMode,
    KeyReference,
    ExportHtml,
    ExportPdf,
    Print,
}

/// The space menu (Helix's `space`, Vim's `\`).
const MENU: &[(char, AppCommand, &str)] = &[
    ('f', AppCommand::OpenFile, "open a file"),
    ('s', AppCommand::GoToHeading, "go to a heading"),
    ('/', AppCommand::Find, "find"),
    ('i', AppCommand::Info, "info panel"),
    ('o', AppCommand::Outline, "outline"),
    ('e', AppCommand::Sidebar, "sidebar of files"),
    ('r', AppCommand::Recent, "recent files"),
    ('d', AppCommand::DarkMode, "light or dark"),
    ('z', AppCommand::FocusMode, "focus mode"),
    ('?', AppCommand::KeyReference, "all keys"),
];

const MENU_HINTS: &[Hint] = &[
    ("f", "open a file"),
    ("s", "go to a heading"),
    ("/", "find"),
    ("i", "info panel"),
    ("o", "outline"),
    ("e", "sidebar of files"),
    ("r", "recent files"),
    ("d", "light or dark"),
    ("z", "focus mode"),
    ("?", "all keys"),
];

/// The menu command behind `key`.
pub(crate) fn menu_command(key: char) -> Option<AppCommand> {
    MENU.iter()
        .find(|(c, _, _)| *c == key)
        .map(|(_, command, _)| *command)
}

pub(crate) fn menu_hints() -> Hints {
    Hints::new("Focal", MENU_HINTS)
}

/// What a `:` command does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ex {
    Save,
    Close,
    SaveClose,
    App(AppCommand),
    Nothing,
}

/// The `:` commands: their names (the first is shown), what they do, and
/// the words for it.
const EX: &[(&[&str], Ex, &str)] = &[
    (&["w", "write", "w!", "up", "update"], Ex::Save, "save"),
    (
        &["q", "quit", "q!", "quit!", "close"],
        Ex::Close,
        "close the window",
    ),
    (
        &["wq", "x", "xit", "wq!", "write-quit"],
        Ex::SaveClose,
        "save and close",
    ),
    (
        &["open", "o", "e", "edit"],
        Ex::App(AppCommand::OpenFile),
        "open a file",
    ),
    (
        &["heading", "h"],
        Ex::App(AppCommand::GoToHeading),
        "go to a heading",
    ),
    (&["info"], Ex::App(AppCommand::Info), "info panel"),
    (&["outline"], Ex::App(AppCommand::Outline), "outline"),
    (
        &["sidebar"],
        Ex::App(AppCommand::Sidebar),
        "sidebar of files",
    ),
    (&["recent"], Ex::App(AppCommand::Recent), "recent files"),
    (
        &["dark", "light"],
        Ex::App(AppCommand::DarkMode),
        "light or dark",
    ),
    (&["focus"], Ex::App(AppCommand::FocusMode), "focus mode"),
    (
        &["html", "export-html"],
        Ex::App(AppCommand::ExportHtml),
        "export as HTML",
    ),
    (
        &["pdf", "export-pdf"],
        Ex::App(AppCommand::ExportPdf),
        "export as PDF",
    ),
    (&["print"], Ex::App(AppCommand::Print), "print"),
    (
        &["keys", "help"],
        Ex::App(AppCommand::KeyReference),
        "all keys",
    ),
    (
        &["noh", "nohlsearch"],
        Ex::Nothing,
        "clear search highlights",
    ),
];

/// Runs a `:` command: a line number goes there; an unknown command says so.
pub(crate) fn run_ex(line: &str, text: &str) -> Result<Vec<Command>, String> {
    let line = line.trim();
    if line.is_empty() {
        return Ok(Vec::new());
    }
    if let Ok(number) = line.parse::<usize>() {
        let at = first_non_blank(text, line_offset(text, number.saturating_sub(1)));
        return Ok(vec![caret(at)]);
    }
    let ex = EX
        .iter()
        .find(|(names, _, _)| names.contains(&line))
        .map(|(_, ex, _)| *ex)
        .ok_or_else(|| format!("Not a command: {line}"))?;
    Ok(match ex {
        Ex::Save => vec![Command::Save],
        Ex::Close => vec![Command::Close],
        Ex::SaveClose => vec![Command::Save, Command::Close],
        Ex::App(command) => vec![Command::App(command)],
        Ex::Nothing => Vec::new(),
    })
}

/// The `:` commands starting with what was typed, with what they do.
pub fn completions(typed: &str) -> Vec<Hint> {
    let typed = typed.trim();
    // Each command under the name that matches what was typed.
    EX.iter()
        .filter_map(|(names, _, does)| {
            names
                .iter()
                .find(|name| name.starts_with(typed))
                .map(|name| (*name, *does))
        })
        .collect()
}

/// Tab: the first command starting with what was typed.
pub(crate) fn complete(typed: &str) -> Option<&'static str> {
    let typed = typed.trim();
    EX.iter().find_map(|(names, _, _)| {
        names
            .iter()
            .find(|name| name.starts_with(typed) && name.len() > typed.len())
            .copied()
    })
}

// ---- Hints for waiting keys -------------------------------------------------

const VIM_MOTIONS: &[Hint] = &[
    ("w b e", "word forward, back, to its end"),
    ("0 ^ $", "line start, first letter, line end"),
    ("j k", "line below, above"),
    ("gg G", "first, last line"),
    ("{ }", "paragraph back, forward"),
    ("f t", "to a letter, up to it"),
    ("iw aw", "inside, around a word"),
    ("is ip", "inside a sentence, paragraph"),
    ("i( i\" …", "inside brackets, quotes"),
];

const VIM_OBJECTS: &[Hint] = &[
    ("w W", "word, WORD"),
    ("s", "sentence"),
    ("p", "paragraph"),
    ("( [ { <", "brackets"),
    ("\" ' `", "quotes"),
];

const VIM_G: &[Hint] = &[
    ("g", "first line"),
    ("e", "end of the previous word"),
    ("j k", "screen line below, above"),
    ("u U ~", "lower, upper, toggle case"),
];

const HELIX_G: &[Hint] = &[
    ("g", "first line"),
    ("e", "last line"),
    ("h l", "line start, end"),
    ("s", "first letter of the line"),
    ("j k", "screen line below, above"),
];

const HELIX_M: &[Hint] = &[
    ("m", "matching bracket"),
    ("i", "select inside…"),
    ("a", "select around…"),
];

const CHARACTER: &[Hint] = &[("any key", "the character")];

/// What a pending Vim command waits for: `operator` while one waits for its
/// motion, `g` and the like after a prefix key.
pub(crate) fn vim_hints(pending: &str, operator: Option<&'static str>) -> Option<Hints> {
    let last = pending.chars().last()?;
    Some(match (operator, last) {
        (Some(_), 'i' | 'a') => Hints::new("text object", VIM_OBJECTS),
        (_, 'f' | 't' | 'F' | 'T' | 'r') => Hints::new("character", CHARACTER),
        (None, 'g') => Hints::new("g", VIM_G),
        (None, '\\') => menu_hints(),
        (Some(name), _) => Hints::new(name, VIM_MOTIONS),
        _ => return None,
    })
}

/// What a pending Helix command waits for.
pub(crate) fn helix_hints(pending: &str) -> Option<Hints> {
    Some(match pending {
        "g" => Hints::new("goto", HELIX_G),
        "m" => Hints::new("match", HELIX_M),
        "mi" => Hints::new("select inside", VIM_OBJECTS),
        "ma" => Hints::new("select around", VIM_OBJECTS),
        " " => menu_hints(),
        _ if pending.ends_with(['f', 't', 'F', 'T', 'r']) => Hints::new("character", CHARACTER),
        _ => return None,
    })
}

// ---- The reference ------------------------------------------------------------

/// A titled group of keys in the reference.
pub type Group = (&'static str, &'static [Hint]);

pub const VIM_REFERENCE: &[Group] = &[
    (
        "Modes",
        &[
            ("i a", "insert before, after the cursor"),
            ("I A", "insert at the line's start, end"),
            ("o O", "open a line below, above"),
            ("v V", "select characters, lines"),
            ("Esc", "back to normal mode"),
        ],
    ),
    (
        "Moving",
        &[
            ("h j k l", "left, down, up, right"),
            ("w b e ge", "by words; W B E by WORDS"),
            ("0 ^ $", "line start, first letter, end"),
            ("gg G 5G", "first, last, fifth line"),
            ("{ }", "paragraph back, forward"),
            ("f t F T", "to a letter on the line; ; , repeat"),
            ("%", "matching bracket"),
            ("⌃D ⌃U", "half a page down, up"),
        ],
    ),
    (
        "Changing",
        &[
            ("d c y", "delete, change, yank + motion"),
            ("dd cc yy", "the whole line"),
            ("> <", "indent, outdent + motion"),
            ("x X s S", "delete, change a letter or line"),
            ("C D Y", "change, delete to the end; yank line"),
            ("p P", "put after, before"),
            ("r ~ J", "replace a letter, toggle case, join"),
            ("gu gU g~", "lower, upper, toggle case + motion"),
            (".", "repeat the last change"),
            ("u ⌃R", "undo, redo"),
        ],
    ),
    (
        "Text objects",
        &[
            ("iw aw", "inside, around a word"),
            ("is as ip ap", "sentence, paragraph"),
            ("i( i[ i{ i<", "inside brackets; a( around"),
            ("i\" i' i`", "inside quotes"),
        ],
    ),
    (
        "Search and commands",
        &[
            ("/ ?", "find forward, backward"),
            ("n N", "next, previous match"),
            ("* #", "the word under the cursor"),
            (":w :q :wq", "save, close, both"),
            (":12", "go to line 12"),
            ("\\", "Focal's menu"),
        ],
    ),
];

pub const HELIX_REFERENCE: &[Group] = &[
    (
        "Modes",
        &[
            ("i a", "insert before, after the selection"),
            ("I A", "insert at the line's start, end"),
            ("o O", "open a line below, above"),
            ("v", "extend the selection while moving"),
            ("Esc", "back to normal mode"),
        ],
    ),
    (
        "Selecting",
        &[
            ("h j k l", "move left, down, up, right"),
            ("w e b", "select to the next word, its end, back"),
            ("x X", "select the line, extend to lines"),
            ("%", "select everything"),
            ("f t F T", "select to a letter"),
            ("mi ma", "select inside, around an object"),
            ("mm", "matching bracket"),
            (";", "collapse to the cursor"),
        ],
    ),
    (
        "Changing the selection",
        &[
            ("d c y", "delete, change, yank"),
            ("p P", "paste after, before"),
            ("R", "replace with what was yanked"),
            ("r ~ `", "replace letters, toggle, lower case"),
            ("> < J", "indent, outdent, join lines"),
            ("u U", "undo, redo"),
            (".", "repeat the last change"),
        ],
    ),
    (
        "Going places",
        &[
            ("gg ge", "first, last line"),
            ("gh gl gs", "line start, end, first letter"),
            ("/ ? n N", "find, next, previous"),
            ("*", "find the selection"),
            (":w :q :wq", "save, close, both"),
            ("space", "Focal's menu"),
        ],
    ),
];

pub const MENU_REFERENCE: Group = ("Focal's menu (space or \\)", MENU_HINTS);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_menu_key_has_a_hint() {
        for (key, _, does) in MENU {
            assert!(
                MENU_HINTS.contains(&(key.to_string().as_str(), does)),
                "{key}"
            );
        }
    }

    #[test]
    fn commands_complete_and_run() {
        assert_eq!(complete("ex"), Some("export-html"));
        assert_eq!(complete("w"), Some("write"));
        assert!(completions("p").iter().any(|(name, _)| *name == "pdf"));
        assert_eq!(
            run_ex("html", ""),
            Ok(vec![Command::App(AppCommand::ExportHtml)])
        );
        assert_eq!(run_ex("wq", ""), Ok(vec![Command::Save, Command::Close]));
        assert_eq!(run_ex("2", "a\nb"), Ok(vec![caret(2)]));
        assert!(run_ex("nope", "").is_err());
    }

    #[test]
    fn hints_follow_the_waiting_keys() {
        assert_eq!(helix_hints("g").map(|h| h.title), Some("goto"));
        assert_eq!(helix_hints("mi").map(|h| h.title), Some("select inside"));
        assert_eq!(helix_hints(" ").map(|h| h.title), Some("Focal"));
        assert_eq!(helix_hints("w"), None);
        assert_eq!(
            vim_hints("d", Some("delete")).map(|h| h.title),
            Some("delete")
        );
        assert_eq!(
            vim_hints("di", Some("delete")).map(|h| h.title),
            Some("text object")
        );
        assert_eq!(vim_hints("2", None), None);
    }
}
