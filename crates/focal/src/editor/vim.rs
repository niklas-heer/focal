//! Vim mode: keys reach [`focal_core::vim::Vim`] before Focal's own key
//! bindings, and Vim's commands run through the editor's edits and moves, so
//! undo, autosave, tables and Markdown-aware typing work as without Vim.

use std::ops::Range;

use focal_core::Bias;
use focal_core::vim::{Command, Context as VimContext, EditorCommand, Key, Mode, Vim};
use gpui_kit::{
    AnyElement, ClipboardItem, Context, InteractiveElement as _, IntoElement as _, Keystroke,
    ParentElement as _, Styled as _, Subscription, TestSupportExt as _, Window, div, px,
};

use super::{CloseWindow, Down, EditKind, Editor, EditorEvent, Redo, Save, Undo, Up};
use crate::find_bar::{Find, FindNext, FindPrevious};
use crate::theme::{MONO_FONT, Theme};

impl Editor {
    /// Gives Vim the keys typed into this editor while Vim mode is on.
    pub(super) fn intercept_vim_keys(cx: &mut Context<Self>) -> Subscription {
        let editor = cx.entity().downgrade();
        cx.intercept_keystrokes(move |event, window, cx| {
            let handled = editor
                .update(cx, |editor, cx| {
                    editor.vim_keystroke(&event.keystroke, window, cx)
                })
                .unwrap_or(false);
            if handled {
                cx.stop_propagation();
            }
        })
    }

    /// Turns Vim mode on, in normal mode, or off.
    pub(super) fn set_vim_mode(&mut self, on: bool, cx: &mut Context<Self>) {
        match (on, self.vim.is_some()) {
            (true, false) => {
                self.vim = Some(Vim::new());
                self.settle_vim(cx);
            }
            (false, true) => {
                self.vim = None;
                self.buffer.end_group();
            }
            _ => {}
        }
    }

    /// Where Vim's block cursor is drawn, if anywhere.
    pub(super) fn vim_block(&self) -> Option<usize> {
        let vim = self.vim.as_ref()?;
        if self.grid.is_some() {
            return None;
        }
        let at = vim.block(self.head())?;
        Some(if vim.mode() == Mode::Normal {
            focal_core::vim::normal_cursor(self.text(), at)
        } else {
            at
        })
    }

    fn vim_keystroke(
        &mut self,
        keystroke: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        // Not while a table cell or an input method has the keys.
        if self.vim.is_none() || !self.focus_handle.is_focused(window) || self.marked.is_some() {
            return false;
        }
        match vim_key(keystroke) {
            Some(key) => self.send_vim_key(key, window, cx),
            None => false,
        }
    }

    /// Hands `key` to Vim and runs what it asks for. Returns whether Vim took
    /// the key.
    fn send_vim_key(&mut self, key: Key, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let clipboard = cx.read_from_clipboard().and_then(|item| item.text());
        let (anchor, head) = (self.tail(), self.head());
        let Some(vim) = self.vim.as_mut() else {
            return false;
        };
        let context = VimContext {
            text: self.buffer.text(),
            anchor,
            head,
            clipboard: clipboard.as_deref(),
        };
        let Some(commands) = vim.key(key, &context) else {
            return false;
        };
        for command in commands {
            self.run_vim(command, window, cx);
        }
        self.settle_vim(cx);
        cx.notify();
        true
    }

    /// Puts the caret on a character in normal mode, out of list and quote
    /// markers.
    fn settle_vim(&mut self, cx: &mut Context<Self>) {
        let Some(vim) = &self.vim else { return };
        if self.grid.is_some() || vim.mode() != Mode::Normal {
            return;
        }
        let (at, _) = vim.settle(self.text(), self.tail(), self.head());
        let at = self.snapshot.analysis.snap(at, Bias::Right);
        if self.selection != (at..at) {
            self.selection = at..at;
            self.reversed = false;
            self.after_selection(cx);
        }
    }

    fn run_vim(&mut self, command: Command, window: &mut Window, cx: &mut Context<Self>) {
        match command {
            Command::Select { anchor, head } => {
                let normal = self
                    .vim
                    .as_ref()
                    .is_some_and(|vim| vim.mode() == Mode::Normal);
                // Visual mode's selection is set exactly, so Vim can tell
                // when the pointer changed it.
                let (anchor, head) = if normal && anchor == head {
                    let at = self.snapshot.analysis.snap(head, Bias::Right);
                    (at, at)
                } else {
                    (anchor, head)
                };
                self.selection = anchor.min(head)..anchor.max(head);
                self.reversed = head < anchor;
                self.goal_x = None;
                self.after_selection(cx);
            }
            Command::Edit { range, text, caret } => {
                self.edit(range, &text, caret..caret, EditKind::Other, cx);
            }
            Command::Editor(EditorCommand::Up) => self.up(&Up, window, cx),
            Command::Editor(EditorCommand::Down) => self.down(&Down, window, cx),
            Command::Editor(EditorCommand::Newline) => self.insert_newline(cx),
            Command::Editor(EditorCommand::Indent { range, outdent }) => {
                self.indent_lines(range, outdent, cx);
            }
            Command::Undo => self.undo(&Undo, window, cx),
            Command::Redo => self.redo(&Redo, window, cx),
            Command::Copy(text) => cx.write_to_clipboard(ClipboardItem::new_string(text)),
            Command::BeginGroup => self.buffer.begin_group(),
            Command::EndGroup => self.buffer.end_group(),
            Command::Find { .. } => window.dispatch_action(Box::new(Find), cx),
            Command::FindNext { forward } => {
                if forward {
                    window.dispatch_action(Box::new(FindNext), cx);
                } else {
                    window.dispatch_action(Box::new(FindPrevious), cx);
                }
            }
            Command::FindWord { word, forward } => cx.emit(EditorEvent::Search {
                query: word,
                forward,
            }),
            Command::Repeat { keys, text } => self.repeat_vim_change(keys, &text, window, cx),
            Command::Save => self.save(&Save, window, cx),
            Command::Close => self.close_window(&CloseWindow, window, cx),
        }
    }

    /// `.`: the last change's keys again, then the text typed after them.
    fn repeat_vim_change(
        &mut self,
        keys: Vec<Key>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(vim) = &mut self.vim {
            vim.set_replaying(true);
        }
        for key in keys {
            self.send_vim_key(key, window, cx);
        }
        if self
            .vim
            .as_ref()
            .is_some_and(|vim| vim.mode() == Mode::Insert)
        {
            for (ix, line) in text.split('\n').enumerate() {
                if ix > 0 {
                    self.insert_newline(cx);
                }
                if !line.is_empty() {
                    self.insert(line, EditKind::Typing, cx);
                }
            }
            self.send_vim_key(Key::Escape, window, cx);
        }
        if let Some(vim) = &mut self.vim {
            vim.set_replaying(false);
        }
    }

    /// `>` and `<`: list items nest and unnest as Tab and ⇧Tab do; other
    /// lines gain or lose a tab (or up to four spaces), as one undo step.
    fn indent_lines(&mut self, range: Range<usize>, outdent: bool, cx: &mut Context<Self>) {
        let lines = &self.snapshot.analysis.lines;
        let first = lines.line_of(range.start);
        let last = lines.line_of(range.end.saturating_sub(1).max(range.start));
        self.buffer.begin_group();
        // From the last line up, so earlier offsets stay put.
        for line in (first..=last).rev() {
            let lines = &self.snapshot.analysis.lines;
            let text = self.buffer.text();
            let start = lines.range(line).start;
            let change = if outdent {
                focal_core::editing::outdent_list_item(text, lines, line).or_else(|| {
                    let rest = &text[start..];
                    let remove = if rest.starts_with('\t') {
                        1
                    } else {
                        rest.bytes().take(4).take_while(|b| *b == b' ').count()
                    };
                    (remove > 0).then(|| focal_core::editing::Change {
                        range: start..start + remove,
                        text: String::new(),
                        selection: start..start,
                    })
                })
            } else {
                focal_core::editing::indent_list_item(text, lines, line).or_else(|| {
                    let empty = lines.range(line).is_empty();
                    (!empty).then(|| focal_core::editing::Change {
                        range: start..start,
                        text: "\t".to_owned(),
                        selection: start..start,
                    })
                })
            };
            if let Some(change) = change {
                self.apply_keeping_selection(&change, cx);
            }
        }
        self.buffer.end_group();
        let start = self.snapshot.analysis.lines.range(first).start;
        let at = self.content_start_at(start);
        self.move_to(at, cx);
    }

    /// The mode and what is being typed (a count, an operator, `:` and its
    /// command), quietly in the bottom corner.
    pub(super) fn render_vim_status(&self, theme: &Theme) -> Option<AnyElement> {
        let vim = self.vim.as_ref()?;
        let label = vim.mode().label().map(|label| format!("-- {label} --"));
        let status = vim.status();
        if label.is_none() && status.is_none() {
            return None;
        }
        let text = [label, status]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join("  ");
        Some(
            div()
                .id("vim-status")
                .test_support()
                .absolute()
                .left(px(16.))
                .bottom(px(12.))
                .px(px(8.))
                .py(px(2.))
                .rounded(px(4.))
                .bg(theme.background)
                .font_family(MONO_FONT)
                .text_size(px(12.))
                .text_color(theme.marker)
                .child(text)
                .into_any_element(),
        )
    }
}

/// The key Vim reads for a keystroke. Keys with ⌘ stay Focal's, and so do
/// arrows and other named keys Vim does not use.
fn vim_key(keystroke: &Keystroke) -> Option<Key> {
    let modifiers = &keystroke.modifiers;
    if modifiers.platform || modifiers.function {
        return None;
    }
    let single = |text: &str| {
        let mut chars = text.chars();
        match (chars.next(), chars.next()) {
            (Some(c), None) => Some(c),
            _ => None,
        }
    };
    if modifiers.control {
        return single(&keystroke.key).map(|c| Key::Ctrl(c.to_ascii_lowercase()));
    }
    match keystroke.key.as_str() {
        "escape" => return Some(Key::Escape),
        "enter" => return Some(Key::Enter),
        "backspace" => return Some(Key::Backspace),
        "tab" => return (!modifiers.shift).then_some(Key::Tab),
        "space" => return Some(Key::Char(' ')),
        _ => {}
    }
    if let Some(c) = keystroke.key_char.as_deref().and_then(single) {
        return Some(Key::Char(c));
    }
    let c = single(&keystroke.key)?;
    Some(Key::Char(if modifiers.shift {
        c.to_ascii_uppercase()
    } else {
        c
    }))
}
