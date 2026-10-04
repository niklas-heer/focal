//! Paste and drop: a URL pasted over selected text becomes a link, rich
//! text becomes Markdown, and images are saved beside the document, in its
//! images folder (`assets/` unless set otherwise).

use std::path::{Path, PathBuf};

use focal_core::analysis::{InlineStyle, LineKind};
use focal_core::paste;
use gpui_kit::{
    AnyElement, App, ClipboardEntry, Context, DragMoveEvent, ExternalPaths,
    InteractiveElement as _, IntoElement as _, ParentElement as _, Styled as _,
    TestSupportExt as _, Window, div, px,
};

use super::{EditKind, Editor, Hit, Paste, PasteAsPlainText};
use crate::settings::Settings;
use crate::theme::Theme;

impl Editor {
    pub(super) fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = cx.read_from_clipboard() else {
            return;
        };
        // Files copied in Finder paste as if they were dropped.
        let paths: Vec<PathBuf> = item
            .entries()
            .iter()
            .filter_map(|entry| match entry {
                ClipboardEntry::ExternalPaths(paths) => Some(paths.paths().to_vec()),
                _ => None,
            })
            .flatten()
            .collect();
        if !paths.is_empty() {
            self.drop_paths(&paths, window, cx);
            return;
        }
        if let Some(plain) = item.text() {
            let text = self.pasted_text(plain);
            self.insert(&text, EditKind::Other, cx);
        } else if let Some(image) = item.entries().iter().find_map(|entry| match entry {
            ClipboardEntry::Image(image) => Some(image),
            _ => None,
        }) {
            self.insert_image(&image.bytes, image.format.extension(), window, cx);
        }
    }

    pub(super) fn paste_plain(
        &mut self,
        _: &PasteAsPlainText,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.insert(&text, EditKind::Other, cx);
        }
    }

    /// What pasting `plain` inserts here: a link over selected text, the
    /// pasteboard's rich text as Markdown in prose, or `plain` itself.
    fn pasted_text(&self, plain: String) -> String {
        if self.pastes_plain_here() {
            return plain;
        }
        let selected = &self.text()[self.selection.clone()];
        if !selected.is_empty() && !selected.contains('\n') && paste::is_url(&plain) {
            return paste::link_over(selected, &plain);
        }
        crate::mac::pasteboard_html()
            .and_then(|html| paste::html_to_markdown(&html, &plain))
            .unwrap_or(plain)
    }

    /// Whether the caret is where Markdown would be taken literally: a table
    /// cell, a code block, math, front matter, HTML, or inline code.
    fn pastes_plain_here(&self) -> bool {
        if self.grid.is_some() {
            return true;
        }
        let head = self.head();
        let analysis = &self.snapshot.analysis;
        let line = analysis.lines.line_of(head);
        if !matches!(
            analysis.info(line).kind,
            LineKind::Text | LineKind::Heading(_)
        ) {
            return true;
        }
        let view = &self.snapshot.views[line];
        let at = view.map.to_display(head);
        let style_at = |offset: usize| {
            let mut start = 0;
            view.runs.iter().find_map(|run| {
                let end = start + run.len;
                let found = (start <= offset && offset < end).then_some(run.style);
                start = end;
                found
            })
        };
        let literal = |style: Option<InlineStyle>| {
            style.is_some_and(|s| s.contains(InlineStyle::CODE) || s.contains(InlineStyle::MATH))
        };
        at > 0 && literal(style_at(at - 1)) && literal(style_at(at))
    }

    /// The document's folder and its images folder, or `None` for an
    /// untitled document, which is asked to be saved first.
    fn asset_folder(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<(PathBuf, PathBuf)> {
        let Some(dir) = self
            .document
            .path
            .as_deref()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
        else {
            self.show_error(
                "Save the document first: images and links are kept beside it.".to_owned(),
                cx,
            );
            self.save_as(false, window, cx);
            return None;
        };
        let folder = cx.global::<Settings>().images_folder.clone();
        let assets = dir.join(folder);
        Some((dir, assets))
    }

    fn insert_image(
        &mut self,
        bytes: &[u8],
        extension: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((dir, assets)) = self.asset_folder(window, cx) else {
            return;
        };
        let name = paste::asset_name(None, extension, now(), |name| assets.join(name).exists());
        let path = assets.join(name);
        if let Err(error) =
            std::fs::create_dir_all(&assets).and_then(|()| std::fs::write(&path, bytes))
        {
            self.show_error(format!("Could not save the image: {error}"), cx);
            return;
        }
        let link = paste::image_markdown(&paste::relative_link(&dir, &path));
        self.insert(&link, EditKind::Other, cx);
    }

    /// Dropped (or pasted) files: images are copied into the images folder,
    /// unless they are beside the document already, and other files are
    /// linked; folders are left out. One undo step, a line per file.
    pub(crate) fn drop_paths(
        &mut self,
        paths: &[PathBuf],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let files: Vec<&PathBuf> = paths.iter().filter(|path| path.is_file()).collect();
        if files.is_empty() {
            return;
        }
        let Some((dir, assets)) = self.asset_folder(window, cx) else {
            return;
        };
        let mut lines = Vec::new();
        for path in files {
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            if paste::is_image(path) {
                let target = if path.starts_with(&dir) {
                    path.clone()
                } else {
                    let name = paste::asset_name(Some(&name), "png", now(), |name| {
                        assets.join(name).exists()
                    });
                    let target = assets.join(&name);
                    let copied = std::fs::create_dir_all(&assets)
                        .and_then(|()| std::fs::copy(path, &target).map(drop));
                    if let Err(error) = copied {
                        self.show_error(format!("Could not copy {name}: {error}"), cx);
                        return;
                    }
                    target
                };
                lines.push(paste::image_markdown(&paste::relative_link(&dir, &target)));
            } else {
                let stem = path
                    .file_stem()
                    .map_or(name.clone(), |stem| stem.to_string_lossy().into_owned());
                lines.push(paste::file_link(&stem, &paste::relative_link(&dir, path)));
            }
        }
        self.insert(&lines.join("\n"), EditKind::Other, cx);
    }
}

impl Editor {
    /// While files are dragged over the editor: what dropping them will do,
    /// and where the pointer is, which is where they land.
    pub(super) fn drag_files(
        &mut self,
        event: &DragMoveEvent<ExternalPaths>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let position = event.event.position;
        let hint = event
            .bounds
            .contains(&position)
            .then(|| drop_hint(event.drag(cx).paths(), cx))
            .flatten()
            .map(|label| (label, position));
        if hint != self.drop_hint {
            self.drop_hint = hint;
            cx.notify();
        }
    }

    pub(super) fn drop_files(
        &mut self,
        paths: &ExternalPaths,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some((_, position)) = self.drop_hint.take()
            && let Some(Hit::Text { offset }) = self.hit_test(position)
        {
            self.move_to(offset, cx);
        }
        self.drop_paths(paths.paths(), window, cx);
    }

    /// The card saying what a drop will do, while files are dragged over.
    pub(super) fn render_drop_hint(&self, theme: &Theme, cx: &App) -> Option<AnyElement> {
        let (label, _) = self.drop_hint.as_ref().filter(|_| cx.has_active_drag())?;
        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .id("drop-hint")
                        .test_support()
                        .px(px(16.))
                        .py(px(10.))
                        .rounded(px(10.))
                        .bg(theme.surface)
                        .border_1()
                        .border_color(theme.rule)
                        .shadow_lg()
                        .text_size(px(13.))
                        .text_color(theme.text)
                        .child(label.clone()),
                )
                .into_any_element(),
        )
    }
}

/// What dropping `paths` does: images copied, other files linked.
pub(crate) fn drop_hint(paths: &[PathBuf], cx: &App) -> Option<String> {
    let files = paths.iter().filter(|path| !path.is_dir());
    let images = files.clone().filter(|path| paste::is_image(path)).count();
    let others = files.count() - images;
    paste::drop_label(images, others, &cx.global::<Settings>().images_folder)
}

/// The local date and time, for naming pasted images.
fn now() -> (i32, u8, u8, u8, u8, u8) {
    use chrono::{Datelike as _, Timelike as _};
    let now = chrono::Local::now();
    #[allow(clippy::cast_possible_truncation)]
    (
        now.year(),
        now.month() as u8,
        now.day() as u8,
        now.hour() as u8,
        now.minute() as u8,
        now.second() as u8,
    )
}
