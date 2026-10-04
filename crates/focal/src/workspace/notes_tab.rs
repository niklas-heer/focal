//! The panel's Notes tab: the review notes beside the document, copying
//! them for an agent, or how to add one.

use focal_core::review::Anchor;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement as _, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div,
    px,
};

use super::Workspace;
use super::panel::{OpenNotesFile, PanelTab, ShowNotes};
use crate::icons::Icon;
use crate::theme::Theme;

impl Workspace {
    /// View ▸ Notes: the review notes beside this file.
    pub(super) fn show_notes(&mut self, _: &ShowNotes, _: &mut Window, cx: &mut Context<Self>) {
        self.panel = Some(PanelTab::Notes);
        cx.notify();
    }

    /// Opens the notes file in this window, to read or edit it as text.
    pub(super) fn open_notes_file(
        &mut self,
        _: &OpenNotesFile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(path) = self
            .editor
            .read(cx)
            .sidecar_path()
            .filter(|path| path.exists())
        {
            self.open_file(path, window, cx);
        }
    }

    fn delete_all_notes(&mut self, cx: &mut Context<Self>) {
        let count = self.editor.read(cx).notes().len();
        let confirmed = crate::mac::confirm(
            &format!("Delete all {count} review notes?"),
            "They are removed from the notes file beside the document.",
            "Delete All",
        );
        if confirmed {
            self.editor
                .update(cx, |editor, cx| editor.delete_all_notes(cx));
        }
    }

    /// The review notes, each with what it is about and says; or how to add
    /// one.
    #[allow(clippy::too_many_lines)]
    pub(super) fn render_notes(&self, theme: &Theme, cx: &Context<Self>) -> AnyElement {
        let editor = self.editor.read(cx);
        let Some(sidecar) = editor.sidecar_name() else {
            return div()
                .pt(px(8.))
                .text_color(theme.marker)
                .child("Save the document to add review notes beside it.")
                .into_any_element();
        };
        let notes = editor.notes();
        if notes.is_empty() {
            let text: SharedString = format!(
                "Select text and choose Add Note (⌥⌘M), or right-click it. Notes are saved \
                 beside the file in {sidecar}, and Copy for Agent hands them to an agent."
            )
            .into();
            return div()
                .id("notes-empty")
                .test_support()
                .aria_label(text.clone())
                .pt(px(8.))
                .text_color(theme.marker)
                .child(text)
                .into_any_element();
        }
        let hover = theme.code_background;
        let button = |id: &'static str, label: &'static str| {
            div()
                .id(id)
                .test_support()
                .aria_label(label)
                .px(px(8.))
                .py(px(3.))
                .rounded(px(6.))
                .border_1()
                .border_color(theme.rule)
                .cursor_pointer()
                .text_size(px(12.))
                .hover(move |style| style.bg(hover))
                .child(label)
        };
        let copy_label = if editor.notes_copied {
            "Copied"
        } else {
            "Copy for Agent"
        };
        let header = div()
            .flex()
            .flex_wrap()
            .gap(px(6.))
            .child(
                button("notes-copy", copy_label)
                    .bg(theme.background)
                    .on_click(|_, window, cx| {
                        window.dispatch_action(Box::new(crate::editor::CopyNotesForAgent), cx);
                    }),
            )
            .child(
                button("notes-open", "Open Notes File").on_click(|_, window, cx| {
                    window.dispatch_action(Box::new(OpenNotesFile), cx);
                }),
            )
            .child(
                button("notes-delete-all", "Delete All")
                    .on_click(cx.listener(|this, _, _, cx| this.delete_all_notes(cx))),
            );
        let anchors = editor.note_anchors();
        let rows = notes.iter().enumerate().map(|(ix, note)| {
            let anchor = anchors.get(ix).cloned();
            let changed = matches!(anchor, Some(Anchor::Changed(_)));
            let at = match anchor {
                Some(Anchor::Found(range)) => range.start,
                Some(Anchor::Changed(at)) => at,
                None => 0,
            };
            let quote = note.quote.replace('\n', " ");
            let label = format!(
                "Line {}: {quote}. {}{}",
                note.line,
                note.text,
                if changed { " (passage changed)" } else { "" }
            );
            let icon_button = |id: (&'static str, usize), label: &'static str, icon: Icon| {
                div()
                    .id(id)
                    .test_support()
                    .aria_label(label)
                    .p(px(2.))
                    .rounded(px(4.))
                    .cursor_pointer()
                    .hover(move |style| style.bg(hover))
                    .child(icon.element(12.).text_color(theme.marker))
            };
            div()
                .id(("note", ix))
                .test_support()
                .aria_label(label)
                .px(px(8.))
                .py(px(6.))
                .rounded(px(6.))
                .cursor_pointer()
                .hover(move |style| style.bg(hover))
                .flex()
                .flex_col()
                .gap(px(3.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .text_size(px(11.))
                        .text_color(theme.marker)
                        .child(format!("Line {}", note.line))
                        .when(changed, |d| {
                            d.child(div().text_color(theme.note).child("passage changed"))
                        })
                        .child(div().flex_1())
                        .child(
                            icon_button(("note-edit", ix), "Edit note", Icon::PenLine).on_click(
                                cx.listener(move |this, _, window, cx| {
                                    cx.stop_propagation();
                                    this.editor
                                        .update(cx, |editor, cx| editor.edit_note(ix, window, cx));
                                }),
                            ),
                        )
                        .child(
                            icon_button(("note-delete", ix), "Delete note", Icon::X).on_click(
                                cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.editor
                                        .update(cx, |editor, cx| editor.delete_note(ix, cx));
                                }),
                            ),
                        ),
                )
                .child(
                    div()
                        .italic()
                        .text_size(px(12.))
                        .text_color(theme.marker)
                        .line_clamp(2)
                        .child(quote),
                )
                .child(div().text_size(px(13.)).child(note.text.clone()))
                .on_click(cx.listener(move |this, _, window, cx| this.jump_to(at, window, cx)))
        });
        div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(header)
            .child(div().flex().flex_col().gap(px(2.)).children(rows))
            .into_any_element()
    }
}
