//! The window's root: the editor, the bottom bar and later the sidebar and
//! quick switcher around it.

use std::time::Duration;

use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Animation, AnimationExt as _, AppContext as _, Context, Entity, Focusable as _,
    InteractiveElement as _, IntoElement, KeyDownEvent, MouseMoveEvent, ParentElement as _, Render,
    Styled as _, Task, TestSupportExt as _, Window, div, px,
};

use crate::bar;
use crate::document::Document;
use crate::editor::Editor;
use crate::theme::Theme;

/// How long the bar stays after the pointer leaves it.
const BAR_LINGER: Duration = Duration::from_secs(1);
const BAR_FADE: Duration = Duration::from_millis(150);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Bar {
    Hidden,
    /// Shown for the n-th time, which names its fade-in animation.
    Shown(u32),
    Leaving(u32),
}

pub struct Workspace {
    editor: Entity<Editor>,
    bar: Bar,
    shows: u32,
    bar_timer: Option<Task<()>>,
}

impl Workspace {
    pub fn new(
        document: Document,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let editor = cx.new(|cx| Editor::new(document, text, window, cx));
        Self {
            editor,
            bar: Bar::Hidden,
            shows: 0,
            bar_timer: None,
        }
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

    fn hide_bar(&mut self, cx: &mut Context<Self>) {
        self.bar_timer = None;
        if self.bar != Bar::Hidden {
            self.bar = Bar::Hidden;
            cx.notify();
        }
    }

    fn pointer_moved(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let from_bottom = window.viewport_size().height - event.position.y;
        let reach = if matches!(self.bar, Bar::Shown(_)) {
            bar::BAR_HEIGHT
        } else {
            bar::SHOW_ZONE
        };
        if from_bottom <= px(reach) {
            self.bar_timer = None;
            if !matches!(self.bar, Bar::Shown(_)) {
                self.shows += 1;
                self.bar = Bar::Shown(self.shows);
                cx.notify();
            }
        } else if matches!(self.bar, Bar::Shown(_)) && self.bar_timer.is_none() {
            self.bar_timer = Some(cx.spawn(async move |this, cx| {
                cx.background_executor().timer(BAR_LINGER).await;
                this.update(cx, |this, cx| {
                    if let Bar::Shown(n) = this.bar {
                        this.bar = Bar::Leaving(n);
                        cx.notify();
                    }
                })
                .ok();
                cx.background_executor().timer(BAR_FADE).await;
                this.update(cx, |this, cx| this.hide_bar(cx)).ok();
            }));
        }
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::for_appearance(window.appearance());
        let editor = self.editor.read(cx);
        let bar = (!editor.focus_mode()).then(|| (self.bar, editor.bar_state()));
        div()
            .id("workspace")
            .relative()
            .size_full()
            .on_mouse_move(cx.listener(Self::pointer_moved))
            // The bar never shows while you type.
            .capture_key_down(cx.listener(|this, _: &KeyDownEvent, _, cx| this.hide_bar(cx)))
            .child(self.editor.clone())
            // Where the pointer shows the bar; tests hover it.
            .child(
                div()
                    .id("bar-zone")
                    .test_support()
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .h(px(bar::SHOW_ZONE)),
            )
            .when_some(bar, |d, (state, bar_state)| match state {
                Bar::Hidden => d,
                Bar::Shown(n) => d.child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .bottom_0()
                        .child(bar::render(&bar_state, &theme))
                        .with_animation(("bar-in", n), Animation::new(BAR_FADE), |el, t| {
                            el.opacity(t)
                        }),
                ),
                Bar::Leaving(n) => d.child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .bottom_0()
                        .child(bar::render(&bar_state, &theme))
                        .with_animation(("bar-out", n), Animation::new(BAR_FADE), |el, t| {
                            el.opacity(1. - t)
                        }),
                ),
            })
    }
}
