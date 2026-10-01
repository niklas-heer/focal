//! Islands: blocks drawn in place of their source lines while the caret is
//! elsewhere (front matter, and later images and display math). A click or
//! the caret moving in shows the source.

use std::hash::{DefaultHasher, Hash as _, Hasher as _};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use focal_core::Analysis;
use focal_core::blocks::{block_image, front_matter, math_blocks};
use focal_core::links;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, Image, ImageFormat, InteractiveElement as _, IntoElement, ObjectFit,
    ParentElement as _, StatefulInteractiveElement as _, Styled as _, StyledImage as _,
    TestSupportExt as _, canvas, div, img, px,
};

use crate::editor::{Editor, Island, IslandKind, PaintedRow, Row};
use crate::math;
use crate::theme::Theme;

/// The islands of this version of the text.
pub(crate) fn islands(analysis: &Analysis, text: &str) -> Vec<Island> {
    let mut islands = Vec::new();
    if let Some(matter) = front_matter(analysis, text) {
        islands.push(Island {
            kind: IslandKind::FrontMatter,
            start: matter.lines.start,
            end: matter.lines.end,
        });
    }
    for block in math_blocks(analysis, text) {
        islands.push(Island {
            kind: IslandKind::Math,
            start: block.lines.start,
            end: block.lines.end,
        });
    }
    for line in 0..analysis.line_count() {
        if block_image(analysis, text, line).is_some() {
            islands.push(Island {
                kind: IslandKind::Image,
                start: line,
                end: line + 1,
            });
        }
    }
    islands
}

/// A formula's typesetting, by its TeX.
#[derive(Clone)]
pub(crate) enum Typeset {
    Pending,
    Svg(String),
    Error(String),
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a color channel in 0..=1 scales to 0..=255"
)]
fn channel(value: f32) -> u8 {
    (value.clamp(0., 1.) * 255.).round() as u8
}

/// Where an image's pixels come from.
enum Picture {
    Ready(PathBuf),
    Loading,
    Missing,
}

/// Downloads `url` into `path` with macOS's `curl`, through a temporary file
/// so a half-written download is never shown.
fn download(url: &str, path: &Path) -> bool {
    let partial = path.with_extension("partial");
    let fetched = std::process::Command::new("curl")
        .args([
            "-fsSL",
            "--max-time",
            "30",
            "--max-filesize",
            "52428800",
            "-o",
        ])
        .arg(&partial)
        .arg(url)
        .status()
        .is_ok_and(|status| status.success());
    fetched && std::fs::rename(&partial, path).is_ok()
}

/// `~/Library/Caches/Focal/images/<hash>.<extension>` for a remote image.
fn cached_path(url: &str) -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let mut hasher = DefaultHasher::new();
    url.hash(&mut hasher);
    let extension = Path::new(url.split(['?', '#']).next().unwrap_or_default())
        .extension()
        .and_then(|e| e.to_str())
        .filter(|e| e.len() <= 5)
        .unwrap_or("img");
    Some(
        PathBuf::from(home)
            .join("Library/Caches/Focal/images")
            .join(format!("{:016x}.{extension}", hasher.finish())),
    )
}

impl Editor {
    /// Where the caret goes when an island is clicked: front matter's first
    /// field, otherwise the island's first line.
    pub(crate) fn island_entry(&self, island: Island) -> usize {
        let line = match island.kind {
            IslandKind::FrontMatter if island.end - island.start > 2 => island.start + 1,
            IslandKind::FrontMatter | IslandKind::Image | IslandKind::Math => island.start,
        };
        self.snapshot.analysis.lines.range(line).start
    }

    /// Where a newly opened document puts the caret: at its body, after any
    /// front matter.
    pub(crate) fn body_start(&self) -> usize {
        let analysis = &self.snapshot.analysis;
        let Some(matter) = front_matter(analysis, self.text()) else {
            return 0;
        };
        (matter.lines.end..analysis.line_count())
            .map(|line| analysis.lines.range(line))
            .find(|range| !self.text()[range.clone()].trim().is_empty())
            .map_or(self.text().len(), |range| range.start)
    }

    pub(crate) fn render_island(
        &self,
        island: Island,
        theme: &Theme,
        cx: &Context<Self>,
    ) -> AnyElement {
        let _ = cx;
        let painted = self.painted.clone();
        let content = match island.kind {
            IslandKind::FrontMatter => self.render_front_matter(theme),
            IslandKind::Image => self.render_image(island.start, "image-island", theme, cx),
            IslandKind::Math => self.render_math(island.start, "math-island", theme, cx),
        };
        div()
            .relative()
            .child(content)
            .child(
                canvas(
                    |_, _, _| {},
                    move |bounds, (), _, _| {
                        painted.borrow_mut().push(PaintedRow {
                            row: Row::Island(island),
                            layout: None,
                            view: None,
                            bounds,
                        });
                    },
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .into_any_element()
    }

    /// The image on `line`, scaled to the column but never past its natural
    /// size; a quiet note while it loads or when it cannot be found.
    pub(crate) fn render_image(
        &self,
        line: usize,
        id: &'static str,
        theme: &Theme,
        cx: &Context<Self>,
    ) -> AnyElement {
        let Some(image) = block_image(&self.snapshot.analysis, self.text(), line) else {
            return div().into_any_element();
        };
        let note = |id: &'static str, text: String| {
            div()
                .id((id, line))
                .test_support()
                .aria_label(text.clone())
                .py(px(6.))
                .text_size(px(self.typography.size * 0.8))
                .text_color(theme.marker)
                .child(text)
                .into_any_element()
        };
        match self.picture(&image.destination, cx) {
            Picture::Ready(path) => {
                let missing = format!("Image not found: {}", image.destination);
                let marker = theme.marker;
                div()
                    .id((id, line))
                    .test_support()
                    .aria_label(image.alt.clone())
                    .py(px(6.))
                    .child(
                        img(path)
                            .max_w_full()
                            .rounded(px(6.))
                            .object_fit(ObjectFit::ScaleDown)
                            .with_fallback(move || {
                                div()
                                    .text_color(marker)
                                    .child(missing.clone())
                                    .into_any_element()
                            }),
                    )
                    .into_any_element()
            }
            Picture::Loading => note("image-loading", format!("Loading {}…", image.destination)),
            Picture::Missing => note(
                "image-missing",
                format!("Image not found: {}", image.destination),
            ),
        }
    }

    /// Finds an image's file: next to the document for a relative path, in
    /// the cache for a remote one, starting its download if needed.
    fn picture(&self, destination: &str, cx: &Context<Self>) -> Picture {
        if destination.starts_with("http://") || destination.starts_with("https://") {
            let Some(path) = cached_path(destination) else {
                return Picture::Missing;
            };
            if path.exists() {
                return Picture::Ready(path);
            }
            if self.failed_images.borrow().contains(destination) {
                return Picture::Missing;
            }
            if self
                .fetching_images
                .borrow_mut()
                .insert(destination.to_owned())
            {
                let url = destination.to_owned();
                cx.spawn(async move |this, cx| {
                    let target = path.clone();
                    let source = url.clone();
                    let fetched = cx
                        .background_executor()
                        .spawn(async move {
                            target
                                .parent()
                                .is_some_and(|dir| std::fs::create_dir_all(dir).is_ok())
                                && download(&source, &target)
                        })
                        .await;
                    this.update(cx, |this, cx| {
                        this.fetching_images.borrow_mut().remove(&url);
                        if !fetched {
                            this.failed_images.borrow_mut().insert(url);
                        }
                        cx.notify();
                    })
                    .ok();
                })
                .detach();
            }
            return Picture::Loading;
        }
        let local = destination.strip_prefix("file://").unwrap_or(destination);
        let path = if Path::new(local).is_absolute() {
            Some(PathBuf::from(links::percent_decode(local)))
        } else {
            self.path()
                .and_then(Path::parent)
                .and_then(|dir| links::relative_target(local, dir))
        };
        match path {
            Some(path) if path.is_file() => Picture::Ready(path),
            _ => Picture::Missing,
        }
    }

    /// The display math starting on `line`, typeset and centered; its TeX
    /// while it is being typeset, and `MathJax`'s message if it cannot be.
    pub(crate) fn render_math(
        &self,
        line: usize,
        id: &'static str,
        theme: &Theme,
        cx: &Context<Self>,
    ) -> AnyElement {
        let Some(block) = math_blocks(&self.snapshot.analysis, self.text())
            .into_iter()
            .find(|block| block.lines.start == line)
        else {
            return div().into_any_element();
        };
        let state = self.math.borrow().get(&block.tex).cloned();
        let size = self.typography.size;
        let frame = div()
            .id((id, line))
            .test_support()
            .aria_label(block.tex.clone())
            .w_full()
            .py(px(8.))
            .flex()
            .justify_center()
            .cursor_pointer();
        let quiet = |text: String| {
            div()
                .text_size(px(size * 0.8))
                .text_color(theme.marker)
                .child(text)
        };
        match state {
            Some(Typeset::Svg(svg)) => {
                let color = theme.text.to_rgb();
                let hex = format!(
                    "#{:02x}{:02x}{:02x}",
                    channel(color.r),
                    channel(color.g),
                    channel(color.b)
                );
                match math::sized(&svg, &hex, size) {
                    Some(sized) => frame
                        .child(
                            img(Arc::new(Image::from_bytes(ImageFormat::Svg, sized.svg)))
                                .w(px(sized.width))
                                .h(px(sized.height)),
                        )
                        .into_any_element(),
                    None => frame.child(quiet(block.tex)).into_any_element(),
                }
            }
            Some(Typeset::Error(message)) => frame
                .child(quiet(format!("Math: {message}")))
                .into_any_element(),
            Some(Typeset::Pending) => frame.child(quiet(block.tex)).into_any_element(),
            None => {
                self.math
                    .borrow_mut()
                    .insert(block.tex.clone(), Typeset::Pending);
                let result = math::typeset(&block.tex);
                let tex = block.tex.clone();
                cx.spawn(async move |this, cx| {
                    let typeset = match result.recv().await {
                        Ok(Ok(svg)) => Typeset::Svg(svg),
                        Ok(Err(message)) => Typeset::Error(message),
                        Err(_) => Typeset::Error("the typesetter stopped".into()),
                    };
                    this.update(cx, |this, cx| {
                        this.math.borrow_mut().insert(tex, typeset);
                        cx.notify();
                    })
                    .ok();
                })
                .detach();
                frame.child(quiet(block.tex)).into_any_element()
            }
        }
    }

    /// Front matter as one quiet line of keys and values.
    fn render_front_matter(&self, theme: &Theme) -> AnyElement {
        let fields = front_matter(&self.snapshot.analysis, self.text())
            .map(|matter| matter.fields)
            .unwrap_or_default();
        let size = self.typography.size;
        div()
            .id("front-matter")
            .test_support()
            .py(px(6.))
            .flex()
            .flex_wrap()
            .gap_x(px(18.))
            .gap_y(px(4.))
            .text_size(px(size * 0.78))
            .cursor_pointer()
            .when(fields.is_empty(), |d| {
                d.child(div().text_color(theme.marker).child("Front matter"))
            })
            .children(fields.into_iter().map(|(key, value)| {
                div()
                    .flex()
                    .gap(px(6.))
                    .child(div().text_color(theme.marker).child(key))
                    .child(div().text_color(theme.text.opacity(0.75)).child(value))
            }))
            .into_any_element()
    }
}
