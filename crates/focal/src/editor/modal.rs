//! Vim and Helix modes: keys reach [`focal_core::modal::Modal`] before
//! Focal's own key bindings, and its commands run through the editor's edits
//! and moves, so undo, autosave, tables and Markdown-aware typing work as
//! without them.

use std::ops::Range;

use focal_core::Bias;
use focal_core::helix::Helix;
use focal_core::modal::Modal;
use focal_core::vim::{Command, Context as KeyContext, EditorCommand, Key, Vim};
use gpui_kit::{
    AnyElement, ClipboardItem, Context, InteractiveElement as _, IntoElement as _, Keystroke,
    ParentElement as _, Styled as _, Subscription, TestSupportExt as _, Window, div, px,
};

use super::{CloseWindow, Down, EditKind, Editor, EditorEvent, Redo, Save, Undo, Up};
use crate::find_bar::{Find, FindNext, FindPrevious};
use crate::settings::Keyboard;
use crate::theme::{MONO_FONT, Theme};

/// The modal keyboard for a setting, if any.
pub(super) fn for_keyboard(keyboard: Keyboard) -> Option<Modal> {
    match keyboard {
        Keyboard::Standard => None,
        Keyboard::Vim => Some(Modal::Vim(Vim::new())),
        Keyboard::Helix => Some(Modal::Helix(Helix::new())),
    }
}

impl Editor {
    /// Gives Vim or Helix the keys typed into this editor while one of them
    /// edits.
    pub(super) fn intercept_modal_keys(cx: &mut Context<Self>) -> Subscription {
        let editor = cx.entity().downgrade();
        cx.intercept_keystrokes(move |event, window, cx| {
            let handled = editor
                .update(cx, |editor, cx| {
                    editor.modal_keystroke(&event.keystroke, window, cx)
                })
                .unwrap_or(false);
            if handled {
                cx.stop_propagation();
            }
        })
    }

    /// Switches between Focal's keys, Vim and Helix; each starts in normal
    /// mode.
    pub(super) fn set_keyboard(&mut self, keyboard: Keyboard, cx: &mut Context<Self>) {
        let current = match &self.modal {
            None => Keyboard::Standard,
            Some(Modal::Vim(_)) => Keyboard::Vim,
            Some(Modal::Helix(_)) => Keyboard::Helix,
        };
        if current == keyboard {
            return;
        }
        self.buffer.end_group();
        self.modal = for_keyboard(keyboard);
        self.settle_modal(cx);
    }

    /// Where the block cursor is drawn, if anywhere.
    pub(super) fn modal_block(&self) -> Option<usize> {
        if self.grid.is_some() {
            return None;
        }
        self.modal.as_ref()?.block(self.text(), self.head())
    }

    fn modal_keystroke(
        &mut self,
        keystroke: &Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        // Not while a table cell or an input method has the keys.
        if self.modal.is_none() || !self.focus_handle.is_focused(window) || self.marked.is_some() {
            return false;
        }
        match modal_key(keystroke) {
            Some(key) => self.send_modal_key(key, window, cx),
            None => false,
        }
    }

    /// Hands `key` to Vim or Helix and runs what it asks for. Returns
    /// whether the key was taken.
    fn send_modal_key(&mut self, key: Key, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let clipboard = cx.read_from_clipboard().and_then(|item| item.text());
        let (anchor, head) = (self.tail(), self.head());
        let Some(modal) = self.modal.as_mut() else {
            return false;
        };
        let context = KeyContext {
            text: self.buffer.text(),
            anchor,
            head,
            clipboard: clipboard.as_deref(),
        };
        let Some(commands) = modal.key(key, &context) else {
            return false;
        };
        for command in commands {
            self.run_modal(command, window, cx);
        }
        self.settle_modal(cx);
        cx.notify();
        true
    }

    /// Puts the selection where the mode wants it after a key: in normal
    /// mode a caret moves out of list and quote markers.
    fn settle_modal(&mut self, cx: &mut Context<Self>) {
        let Some(modal) = &self.modal else { return };
        if self.grid.is_some() {
            return;
        }
        let (anchor, head) = modal.settle(self.text(), self.tail(), self.head());
        let (anchor, head) = if anchor == head && modal.normal() {
            let at = self.snapshot.analysis.snap(head, Bias::Right);
            (at, at)
        } else {
            (anchor, head)
        };
        if (self.tail(), self.head()) != (anchor, head) {
            self.selection = anchor.min(head)..anchor.max(head);
            self.reversed = head < anchor;
            self.after_selection(cx);
        }
    }

    fn run_modal(&mut self, command: Command, window: &mut Window, cx: &mut Context<Self>) {
        match command {
            Command::Select { anchor, head } => {
                let normal = self.modal.as_ref().is_some_and(Modal::normal);
                // Selections are set exactly, so Vim and Helix can tell when
                // the pointer changed them.
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
            Command::Repeat { keys, text } => self.repeat_modal_change(keys, &text, window, cx),
            Command::Save => self.save(&Save, window, cx),
            Command::Close => self.close_window(&CloseWindow, window, cx),
        }
    }

    /// `.`: the last change's keys again, then the text typed after them.
    fn repeat_modal_change(
        &mut self,
        keys: Vec<Key>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(modal) = &mut self.modal {
            modal.set_replaying(true);
        }
        for key in keys {
            self.send_modal_key(key, window, cx);
        }
        if self.modal.as_ref().is_some_and(Modal::inserting) {
            for (ix, line) in text.split('\n').enumerate() {
                if ix > 0 {
                    self.insert_newline(cx);
                }
                if !line.is_empty() {
                    self.insert(line, EditKind::Typing, cx);
                }
            }
            self.send_modal_key(Key::Escape, window, cx);
        }
        if let Some(modal) = &mut self.modal {
            modal.set_replaying(false);
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
    pub(super) fn render_modal_status(&self, theme: &Theme) -> Option<AnyElement> {
        let modal = self.modal.as_ref()?;
        let label = modal.label().map(|label| format!("-- {label} --"));
        let status = modal.status();
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
                .id("modal-status")
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

/// The key Vim and Helix read for a keystroke. Keys with ⌘ stay Focal's,
/// and so do arrows and other named keys they do not use.
fn modal_key(keystroke: &Keystroke) -> Option<Key> {
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
