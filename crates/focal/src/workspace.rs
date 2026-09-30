//! The window's root: the editor, and later the sidebar, bottom bar and quick
//! switcher around it.

use gpui_kit::{
    AppContext as _, Context, Entity, Focusable as _, IntoElement, ParentElement as _, Render,
    Styled as _, Window, div,
};

use crate::document::Document;
use crate::editor::Editor;

pub struct Workspace {
    editor: Entity<Editor>,
}

impl Workspace {
    pub fn new(
        document: Document,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let editor = cx.new(|cx| Editor::new(document, text, window, cx));
        Self { editor }
    }

    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "used by tests until folder mode")
    )]
    pub fn editor(&self) -> &Entity<Editor> {
        &self.editor
    }

    /// Gives the editor keyboard focus.
    pub fn focus_editor(this: &Entity<Self>, window: &mut Window, cx: &mut gpui_kit::App) {
        let handle = this.read(cx).editor.read(cx).focus_handle(cx);
        window.focus(&handle, cx);
    }
}

impl Render for Workspace {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.editor.clone())
    }
}
