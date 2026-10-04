//! The modal keyboards Focal offers beside its own: Vim and Helix, behind one
//! face for the editor. Both read [`Key`]s and answer with [`Command`]s.

use crate::helix::{self, Helix};
use crate::vim::{self, Command, Context, Key, Vim};

#[derive(Debug)]
pub enum Modal {
    Vim(Vim),
    Helix(Helix),
}

impl Modal {
    /// Handles a key; `None` leaves it to the editor, as typing while
    /// inserting.
    pub fn key(&mut self, key: Key, cx: &Context) -> Option<Vec<Command>> {
        match self {
            Self::Vim(vim) => vim.key(key, cx),
            Self::Helix(helix) => helix.key(key, cx),
        }
    }

    /// The mode's name to show.
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Vim(vim) => vim.mode().label(),
            Self::Helix(helix) => helix.mode().label(),
        }
    }

    /// The command line, a message, or the keys of a command being typed.
    pub fn status(&self) -> Option<String> {
        match self {
            Self::Vim(vim) => vim.status(),
            Self::Helix(helix) => helix.status(),
        }
    }

    /// What the keys typed so far wait for: the keys that finish the
    /// command, and what they do.
    pub fn hints(&self) -> Option<crate::keys::Hints> {
        match self {
            Self::Vim(vim) => vim.hints(),
            Self::Helix(helix) => helix.hints(),
        }
    }

    /// The `:` commands that match what was typed after `:`, while the
    /// command line is open.
    pub fn completions(&self) -> Option<Vec<crate::keys::Hint>> {
        let line = match self {
            Self::Vim(vim) => vim.command_line(),
            Self::Helix(helix) => helix.command_line(),
        }?;
        // A line number needs no list.
        if line.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            return None;
        }
        Some(crate::keys::completions(line))
    }

    /// The key that opens Focal's menu.
    pub const fn menu_key(&self) -> &'static str {
        match self {
            Self::Vim(_) => "\\",
            Self::Helix(_) => "space",
        }
    }

    /// A query searched for with the find bar, for `n` to find again.
    pub fn remember_search(&mut self, query: &str) {
        match self {
            Self::Vim(vim) => vim.remember_search(query),
            Self::Helix(helix) => helix.remember_search(query),
        }
    }

    /// Which reference describes these keys.
    pub const fn reference(&self) -> &'static [crate::keys::Group] {
        match self {
            Self::Vim(_) => crate::keys::VIM_REFERENCE,
            Self::Helix(_) => crate::keys::HELIX_REFERENCE,
        }
    }

    pub fn inserting(&self) -> bool {
        match self {
            Self::Vim(vim) => vim.mode() == vim::Mode::Insert,
            Self::Helix(helix) => helix.mode() == helix::Mode::Insert,
        }
    }

    /// Whether the cursor is a caret in normal mode, which the editor may
    /// move out of list and quote markers.
    pub fn normal(&self) -> bool {
        match self {
            Self::Vim(vim) => vim.mode() == vim::Mode::Normal,
            Self::Helix(helix) => helix.mode() == helix::Mode::Normal,
        }
    }

    /// Where the block cursor is drawn, if anywhere.
    pub fn block(&self, text: &str, head: usize) -> Option<usize> {
        match self {
            Self::Vim(vim) => {
                let at = vim.block(head)?;
                Some(if vim.mode() == vim::Mode::Normal {
                    vim::normal_cursor(text, at)
                } else {
                    at
                })
            }
            Self::Helix(helix) => helix.block(head),
        }
    }

    /// The selection after a key's commands ran: Vim's normal mode keeps
    /// the caret on a character; Helix leaves it as it is.
    pub fn settle(&self, text: &str, anchor: usize, head: usize) -> (usize, usize) {
        match self {
            Self::Vim(vim) => vim.settle(text, anchor, head),
            Self::Helix(_) => (anchor, head),
        }
    }

    pub fn typed(&mut self, text: &str) {
        match self {
            Self::Vim(vim) => vim.typed(text),
            Self::Helix(helix) => helix.typed(text),
        }
    }

    pub fn backspaced(&mut self) {
        match self {
            Self::Vim(vim) => vim.backspaced(),
            Self::Helix(helix) => helix.backspaced(),
        }
    }

    /// Leaves visual or select mode, as when the pointer starts a selection.
    pub fn reset(&mut self) {
        match self {
            Self::Vim(vim) => vim.reset(),
            Self::Helix(helix) => helix.reset(),
        }
    }

    pub const fn set_replaying(&mut self, replaying: bool) {
        match self {
            Self::Vim(vim) => vim.set_replaying(replaying),
            Self::Helix(helix) => helix.set_replaying(replaying),
        }
    }
}
