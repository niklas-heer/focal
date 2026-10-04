//! Review notes for agents: a passage and what to change, kept in
//! `NAME.review.md` beside the document (never in it), drawn as an
//! underline and a dot in the right margin, and listed in the Notes tab.

use std::ops::Range;
use std::path::PathBuf;

use focal_core::review::{self, Anchor, Note};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::{
    AnyElement, AppContext as _, ClipboardItem, Context, Entity, InteractiveElement as _,
    IntoElement as _, ParentElement as _, Pixels, Point, Styled as _, Subscription,
    TestSupportExt as _, Window, div, point, px,
};

use super::{AddNote, CancelNote, CopyNotesForAgent, Editor, Row};
use crate::theme::{PROSE_FONT, Theme};

/// The note being written or edited, in its card.
pub(crate) struct NoteDraft {
    /// The passage, or the caret's line.
    range: Range<usize>,
    /// The note being edited, or `None` for a new one.
    editing: Option<usize>,
    input: Entity<InputState>,
    _events: Subscription,
}

impl Editor {
    fn sidecar(&self) -> Option<PathBuf> {
        self.document.path.as_deref().map(review::sidecar_path)
    }

    /// Reads the notes beside the document.
    pub(super) fn load_notes(&mut self) {
        let parsed = self
            .sidecar()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .map(|text| review::parse(&text))
            .unwrap_or_default();
        self.notes = parsed.notes;
        self.notes_extra = parsed.extra;
        self.anchor_notes();
    }

    /// Reads the notes again whenever their file changes on disk, as when
    /// an agent deletes the notes it handled.
    pub(super) fn watch_notes(&mut self, cx: &mut Context<Self>) {
        self.notes_watch = None;
        let Some(path) = self.sidecar().filter(|_| crate::document::WATCH_FILES) else {
            return;
        };
        let Ok((watcher, events)) = crate::document::watch(&path) else {
            return;
        };
        let task = cx.spawn(async move |this, cx| {
            while events.recv().await.is_ok() {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(200))
                    .await;
                while events.try_recv().is_ok() {}
                if this.update(cx, |this, cx| this.reload_notes(cx)).is_err() {
                    break;
                }
            }
        });
        self.notes_watch = Some((watcher, task));
    }

    /// Reads the notes again, as after an agent changed them.
    pub(crate) fn reload_notes(&mut self, cx: &mut Context<Self>) {
        self.load_notes();
        cx.emit(super::EditorEvent::NotesChanged);
        cx.notify();
    }

    /// Writes the notes beside the document, or removes the file with the
    /// last of them.
    fn save_notes(&mut self, cx: &mut Context<Self>) {
        self.anchor_notes();
        let Some(path) = self.sidecar() else {
            return;
        };
        let result = if self.notes.is_empty() && self.notes_extra.trim().is_empty() {
            match std::fs::remove_file(&path) {
                Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
                _ => Ok(()),
            }
        } else {
            let name = self
                .document
                .path
                .as_deref()
                .and_then(|path| path.file_name())
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            std::fs::write(&path, review::write(&name, &self.notes, &self.notes_extra))
        };
        if let Err(error) = result {
            self.show_error(format!("Could not save the review notes: {error}"), cx);
        }
        cx.emit(super::EditorEvent::NotesChanged);
        cx.notify();
    }

    pub(crate) fn notes(&self) -> &[Note] {
        &self.notes
    }

    /// Where each note's passage is now.
    pub(crate) fn note_anchors(&self) -> &[Anchor] {
        &self.note_anchors
    }

    /// Finds the notes' passages again, after an edit or a reload.
    pub(super) fn anchor_notes(&mut self) {
        self.note_anchors = self
            .notes
            .iter()
            .map(|note| review::anchor(self.text(), note))
            .collect();
    }

    /// Edit ▸ Add Note…: a note on the selection, or on the caret's line.
    pub(super) fn add_note(&mut self, _: &AddNote, window: &mut Window, cx: &mut Context<Self>) {
        if self.document.path.is_none() {
            self.show_error(
                "Save the document to add review notes beside it.".to_owned(),
                cx,
            );
            return;
        }
        let range = if self.selection.is_empty() {
            let analysis = &self.snapshot.analysis;
            analysis.content_range(analysis.lines.line_of(self.head()))
        } else {
            self.selection.clone()
        };
        self.open_note_draft(range, None, String::new(), window, cx);
    }

    /// Opens the card for note `ix`, to change what it says.
    pub(crate) fn edit_note(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(note), Some(anchor)) = (self.notes.get(ix), self.note_anchors.get(ix)) else {
            return;
        };
        let range = match anchor {
            Anchor::Found(range) => range.clone(),
            Anchor::Changed(at) => *at..*at,
        };
        let text = note.text.clone();
        self.open_note_draft(range, Some(ix), text, window, cx);
    }

    fn open_note_draft(
        &mut self,
        range: Range<usize>,
        editing: Option<usize>,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Note for the agent…"));
        input.update(cx, |input, cx| {
            input.set_value(text, window, cx);
            input.focus(window, cx);
        });
        let events = cx.subscribe_in(&input, window, |this, _, event: &InputEvent, window, cx| {
            if let InputEvent::PressEnter { .. } = event {
                this.commit_note(window, cx);
            }
        });
        self.note_draft = Some(NoteDraft {
            range,
            editing,
            input,
            _events: events,
        });
        // The workspace draws the card.
        cx.emit(super::EditorEvent::NotesChanged);
        cx.notify();
    }

    fn commit_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(draft) = self.note_draft.take() else {
            return;
        };
        let text = draft.input.read(cx).value().trim().to_owned();
        window.focus(&self.focus_handle, cx);
        if text.is_empty() {
            cx.emit(super::EditorEvent::NotesChanged);
            cx.notify();
            return;
        }
        if let Some(ix) = draft.editing {
            if let Some(note) = self.notes.get_mut(ix) {
                note.text = text;
            }
        } else {
            let line = self.snapshot.analysis.lines.line_of(draft.range.start) + 1;
            let quote = self.text()[draft.range.clone()].trim().to_owned();
            self.notes.push(Note { line, quote, text });
            self.notes.sort_by_key(|note| note.line);
        }
        self.save_notes(cx);
    }

    pub(crate) fn cancel_note(
        &mut self,
        _: &CancelNote,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.note_draft.take().is_some() {
            window.focus(&self.focus_handle, cx);
            cx.emit(super::EditorEvent::NotesChanged);
            cx.notify();
        }
    }

    pub(crate) fn delete_note(&mut self, ix: usize, cx: &mut Context<Self>) {
        if ix < self.notes.len() {
            self.notes.remove(ix);
            self.save_notes(cx);
        }
    }

    pub(crate) fn delete_all_notes(&mut self, cx: &mut Context<Self>) {
        self.notes.clear();
        self.save_notes(cx);
    }

    /// Edit ▸ Copy Notes for Agent: a prompt with every note.
    pub(super) fn copy_notes_for_agent(
        &mut self,
        _: &CopyNotesForAgent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(name) = self
            .document
            .path
            .as_deref()
            .and_then(|path| path.file_name())
            .map(|name| name.to_string_lossy().into_owned())
        else {
            return;
        };
        if self.notes.is_empty() {
            self.show_error("There are no review notes to copy yet.".to_owned(), cx);
            return;
        }
        cx.write_to_clipboard(ClipboardItem::new_string(review::prompt(
            &name,
            &self.notes,
        )));
        self.notes_copied = true;
        cx.emit(super::EditorEvent::NotesChanged);
        cx.notify();
    }

    /// The sidecar's file name, for the Notes tab.
    pub(crate) fn sidecar_name(&self) -> Option<String> {
        self.sidecar().and_then(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
    }

    pub(crate) fn sidecar_path(&self) -> Option<PathBuf> {
        self.sidecar()
    }

    /// The noted passages on the line `range`, as source ranges.
    pub(super) fn noted_on(&self, line: &Range<usize>) -> Vec<Range<usize>> {
        self.note_anchors
            .iter()
            .filter_map(|anchor| match anchor {
                Anchor::Found(range) if range.start <= line.end && line.start <= range.end => {
                    Some(range.start.max(line.start)..range.end.min(line.end))
                }
                _ => None,
            })
            .filter(|range| !range.is_empty())
            .collect()
    }

    /// The notes whose dot goes beside the line `range`: those starting on
    /// it, with whether their passage changed.
    pub(super) fn note_dots(&self, line: &Range<usize>) -> Vec<(usize, bool)> {
        self.note_anchors
            .iter()
            .enumerate()
            .filter_map(|(ix, anchor)| {
                let (at, changed) = match anchor {
                    Anchor::Found(range) => (range.start, false),
                    Anchor::Changed(at) => (*at, true),
                };
                (line.start <= at && at <= line.end).then_some((ix, changed))
            })
            .collect()
    }

    /// Where `offset` was drawn in the last frame, below its line.
    fn point_below(&self, offset: usize) -> Option<Point<Pixels>> {
        let line = self.snapshot.analysis.lines.line_of(offset);
        let painted = self.painted.borrow();
        let row = painted.iter().find(|row| row.row == Row::Line(line))?;
        let (layout, view) = (row.layout.as_ref()?, row.view.as_ref()?);
        let position = layout.position_for_index(view.map.to_display(offset))?;
        Some(point(position.x, position.y + layout.line_height()))
    }

    /// The card a note is written in, under its passage, in window
    /// coordinates. The workspace draws it, outside the editor, so the
    /// editor's keys (Return, Tab, …) stay out of the note.
    pub(crate) fn render_note_draft(&self, theme: &Theme) -> Option<AnyElement> {
        let draft = self.note_draft.as_ref()?;
        let frame = self.frame.get();
        let width = px(360.);
        let (left, top) = match self.point_below(draft.range.start) {
            Some(at) => (
                at.x.min(frame.right() - width - px(16.))
                    .max(frame.left() + px(16.)),
                at.y + px(6.),
            ),
            None => (
                frame.left() + (frame.size.width - width) / 2.,
                frame.top() + frame.size.height / 3.,
            ),
        };
        let title = if draft.editing.is_some() {
            "Edit note"
        } else {
            "Note for the agent"
        };
        Some(
            div()
                .id("note-draft")
                .test_support()
                .key_context("FocalNote")
                .occlude()
                .absolute()
                .left(left)
                .top(top)
                .w(width)
                .p(px(8.))
                .flex()
                .flex_col()
                .gap(px(4.))
                .bg(theme.surface)
                .border_1()
                .border_color(theme.note)
                .rounded(px(8.))
                .shadow_lg()
                .font_family(PROSE_FONT)
                .text_size(px(13.))
                .text_color(theme.text)
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(theme.marker)
                        .child(format!("{title} · Return saves, Esc cancels")),
                )
                .child(Input::new(&draft.input).appearance(false))
                .into_any_element(),
        )
    }
}
