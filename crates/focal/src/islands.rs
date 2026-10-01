//! Islands: blocks drawn in place of their source lines while the caret is
//! elsewhere (front matter, and later images and display math). A click or
//! the caret moving in shows the source.

use std::hash::{DefaultHasher, Hash as _, Hasher as _};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use focal_core::Analysis;
use focal_core::analysis::Alert;
use focal_core::blocks::{block_image, diagram_blocks, front_matter, math_blocks};
use focal_core::links;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, Image, ImageFormat, InteractiveElement as _, IntoElement, ObjectFit,
    ParentElement as _, StatefulInteractiveElement as _, Styled as _, StyledImage as _,
    TestSupportExt as _, canvas, div, img, px,
};

use crate::editor::{Editor, Island, IslandKind, PaintedRow, Row};
use crate::theme::Theme;
use crate::{diagram, math};

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
    for block in diagram_blocks(analysis, text) {
        islands.push(Island {
            kind: IslandKind::Diagram,
            start: block.lines.start,
            end: block.lines.end,
        });
    }
    for block in math_blocks(analysis, text) {
        islands.push(Island {
            kind: IslandKind::Math,
            start: block.lines.start,
            end: block.lines.end,
        });
    }
    // Only lines that hold an image can be image islands.
    for line in analysis
        .images
        .iter()
        .map(|image| analysis.lines.line_of(image.range.start))
    {
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

/// What a math or diagram island shows.
#[derive(Debug, PartialEq, Eq)]
enum Shown<'a> {
    Svg(&'a str),
    Pending,
    Error(&'a str),
}

/// The current picture, or while a changed block renders, its last one; an
/// error always shows, so a stale picture never passes for current.
fn shown<'a>(state: Option<&'a Typeset>, last: Option<&'a String>) -> Shown<'a> {
    match state {
        Some(Typeset::Svg(svg)) => Shown::Svg(svg),
        Some(Typeset::Error(message)) => Shown::Error(message),
        Some(Typeset::Pending) | None => last.map_or(Shown::Pending, |svg| Shown::Svg(svg)),
    }
}

/// A math or diagram island: its content, centered.
fn island_frame(
    id: &'static str,
    line: usize,
    label: String,
    content: impl IntoElement,
) -> AnyElement {
    div()
        .id((id, line))
        .test_support()
        .aria_label(label)
        .w_full()
        .py(px(8.))
        .flex()
        .justify_center()
        .cursor_pointer()
        .child(content)
        .into_any_element()
}

/// A quiet note in place of a picture.
fn quiet(text: String, size: f32, theme: &Theme) -> impl IntoElement {
    div()
        .text_size(px(size * 0.8))
        .text_color(theme.marker)
        .child(text)
}

/// Focal's colors for a diagram in this appearance.
fn palette(theme: &Theme) -> diagram::Palette {
    let hex = |color: gpui_kit::Hsla| {
        let rgb = color.to_rgb();
        let blend = |c: f32, base: f32| c * rgb.a + base * (1. - rgb.a);
        let base = theme.background.to_rgb();
        format!(
            "#{:02x}{:02x}{:02x}",
            channel(blend(rgb.r, base.r)),
            channel(blend(rgb.g, base.g)),
            channel(blend(rgb.b, base.b))
        )
    };
    diagram::Palette {
        canvas: hex(theme.background),
        surface: hex(theme.code_background),
        text: hex(theme.text),
        line: hex(theme.marker),
        // Charts take Focal's accents, softened toward the page.
        series: [
            theme.link,
            theme.alert(Alert::Tip),
            theme.alert(Alert::Warning),
            theme.alert(Alert::Important),
            theme.alert(Alert::Caution),
            theme.alert(Alert::Note),
        ]
        .into_iter()
        .map(|color| hex(color.opacity(0.75)))
        .collect(),
    }
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a color channel in 0..=1 scales to 0..=255"
)]
fn channel(value: f32) -> u8 {
    (value.clamp(0., 1.) * 255.).round() as u8
}

/// Tall images are scaled down to this height, so one never fills the window.
const MAX_IMAGE_HEIGHT: f32 = 560.;

/// The size to draw an image of `width` × `height` pixels (one pixel a
/// point): scaled down, keeping its shape, to fit `max_width` × `max_height`.
fn fit(width: f32, height: f32, max_width: f32, max_height: f32) -> (f32, f32) {
    let scale = (max_width / width).min(max_height / height).min(1.);
    (width * scale, height * scale)
}

/// Typeset formulas and drawn diagrams kept at most: every keystroke in a
/// block being edited makes a new entry.
const CACHE_LIMIT: usize = 64;

/// Inserts into a cache, starting it over when it is full.
fn remember<K: Eq + std::hash::Hash, V>(
    cache: &mut std::collections::HashMap<K, V>,
    key: K,
    value: V,
) {
    if cache.len() >= CACHE_LIMIT && !cache.contains_key(&key) {
        cache.clear();
    }
    cache.insert(key, value);
}

/// How long a failed download waits before the next try.
const RETRY_AFTER: Duration = Duration::from_mins(1);

/// Whether a download that failed at `failed` may be tried again.
fn retry_due(failed: Instant, now: Instant) -> bool {
    now.duration_since(failed) >= RETRY_AFTER
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
            // A diagram opens at its first line of source, inside the fences.
            IslandKind::Diagram if island.end - island.start > 1 => island.start + 1,
            IslandKind::FrontMatter
            | IslandKind::Image
            | IslandKind::Math
            | IslandKind::Diagram => island.start,
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
            IslandKind::Diagram => self.render_diagram(island.start, "diagram-island", theme, cx),
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
                        img(path.clone())
                            .map(|image| {
                                // GPUI sizes an image to its pixels; size it to fit instead.
                                match imagesize::size(&path) {
                                    Ok(size) => {
                                        let (width, height) = fit(
                                            size.width as f32,
                                            size.height as f32,
                                            self.typography.column,
                                            MAX_IMAGE_HEIGHT,
                                        );
                                        image.w(px(width)).h(px(height))
                                    }
                                    Err(_) => image.max_w_full(),
                                }
                            })
                            .rounded(px(6.))
                            .object_fit(ObjectFit::Contain)
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
            let failed = self.failed_images.borrow().get(destination).copied();
            if let Some(failed) = failed {
                if !retry_due(failed, Instant::now()) {
                    return Picture::Missing;
                }
                self.failed_images.borrow_mut().remove(destination);
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
                            this.failed_images.borrow_mut().insert(url, Instant::now());
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

    /// The display math starting on `line`, typeset and centered; while a
    /// changed formula is typeset, its last picture (or its TeX); the typesetter's
    /// message if it cannot be typeset.
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
        if state.is_none() {
            self.typeset_math(block.tex.clone(), cx);
        }
        let last = self.remember_picture(("math", line), state.as_ref());
        let size = self.typography.size;
        let content = match shown(state.as_ref(), last.as_ref()) {
            Shown::Svg(svg) => {
                let color = theme.text.to_rgb();
                let hex = format!(
                    "#{:02x}{:02x}{:02x}",
                    channel(color.r),
                    channel(color.g),
                    channel(color.b)
                );
                match math::sized(svg, &hex, size) {
                    Some(sized) => img(Arc::new(Image::from_bytes(ImageFormat::Svg, sized.svg)))
                        .w(px(sized.width))
                        .h(px(sized.height))
                        .into_any_element(),
                    None => quiet(block.tex.clone(), size, theme).into_any_element(),
                }
            }
            Shown::Error(message) => {
                quiet(format!("Math: {message}"), size, theme).into_any_element()
            }
            Shown::Pending => quiet(block.tex.clone(), size, theme).into_any_element(),
        };
        island_frame(id, line, block.tex, content)
    }

    fn typeset_math(&self, tex: String, cx: &Context<Self>) {
        remember(&mut self.math.borrow_mut(), tex.clone(), Typeset::Pending);
        let source = tex.clone();
        let result = cx
            .background_executor()
            .spawn(async move { math::typeset(&source) });
        cx.spawn(async move |this, cx| {
            let typeset = match result.await {
                Ok(svg) => Typeset::Svg(svg),
                Err(message) => Typeset::Error(message),
            };
            this.update(cx, |this, cx| {
                remember(&mut this.math.borrow_mut(), tex, typeset);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Keeps a block's newest picture, and returns the one it had before.
    fn remember_picture(
        &self,
        block: (&'static str, usize),
        state: Option<&Typeset>,
    ) -> Option<String> {
        let last = self.last_pictures.borrow().get(&block).cloned();
        if let Some(Typeset::Svg(svg)) = state {
            remember(&mut self.last_pictures.borrow_mut(), block, svg.clone());
        }
        last
    }

    /// The Mermaid diagram whose block starts on `line`, scaled to fit the
    /// column; while a changed diagram is drawn, its last picture; the
    /// parser's message if it cannot be drawn.
    pub(crate) fn render_diagram(
        &self,
        line: usize,
        id: &'static str,
        theme: &Theme,
        cx: &Context<Self>,
    ) -> AnyElement {
        let Some(block) = diagram_blocks(&self.snapshot.analysis, self.text())
            .into_iter()
            .find(|block| block.lines.start == line)
        else {
            return div().into_any_element();
        };
        let key = (block.source.clone(), palette(theme));
        let state = self.diagrams.borrow().get(&key).cloned();
        if state.is_none() {
            self.draw_diagram(key, cx);
        }
        let last = self.remember_picture(("diagram", line), state.as_ref());
        let size = self.typography.size;
        let content = match shown(state.as_ref(), last.as_ref()) {
            Shown::Svg(svg) => match diagram::svg_size(svg) {
                Some((width, height)) => {
                    let (width, height) =
                        fit(width, height, self.typography.column, MAX_IMAGE_HEIGHT);
                    let sharp = diagram::with_size(svg, width * 2., height * 2.);
                    img(Arc::new(Image::from_bytes(
                        ImageFormat::Svg,
                        sharp.into_bytes(),
                    )))
                    .w(px(width))
                    .h(px(height))
                    .into_any_element()
                }
                None => quiet("Mermaid diagram".into(), size, theme).into_any_element(),
            },
            Shown::Error(message) => {
                quiet(format!("Mermaid: {message}"), size, theme).into_any_element()
            }
            Shown::Pending => quiet("Drawing the diagram…".into(), size, theme).into_any_element(),
        };
        island_frame(id, line, "Mermaid diagram".into(), content)
    }

    fn draw_diagram(&self, key: (String, diagram::Palette), cx: &Context<Self>) {
        remember(
            &mut self.diagrams.borrow_mut(),
            key.clone(),
            Typeset::Pending,
        );
        let failed = key.clone();
        let task = cx
            .background_executor()
            .spawn(async move { diagram::render(&key.0, &key.1).map(|svg| (key, svg)) });
        cx.spawn(async move |this, cx| {
            let (key, typeset) = match task.await {
                Ok((key, svg)) => (key, Typeset::Svg(svg)),
                Err(message) => (failed, Typeset::Error(message)),
            };
            this.update(cx, |this, cx| {
                remember(&mut this.diagrams.borrow_mut(), key, typeset);
                cx.notify();
            })
            .ok();
        })
        .detach();
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

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{CACHE_LIMIT, Shown, Typeset, fit, remember, retry_due, shown};

    #[test]
    fn images_fit_the_column_and_the_height_cap_keeping_their_shape() {
        assert_eq!(
            fit(400., 300., 720., 560.),
            (400., 300.),
            "small images keep their size"
        );
        assert_eq!(
            fit(1440., 900., 720., 560.),
            (720., 450.),
            "wide images fit the column"
        );
        assert_eq!(
            fit(600., 2400., 720., 560.),
            (140., 560.),
            "tall images fit the cap"
        );
    }

    #[test]
    fn caches_forget_old_entries_past_their_limit() {
        let mut cache = std::collections::HashMap::new();
        for n in 0..CACHE_LIMIT {
            remember(&mut cache, n, n);
        }
        assert_eq!(cache.len(), CACHE_LIMIT);
        remember(&mut cache, CACHE_LIMIT, CACHE_LIMIT);
        assert_eq!(
            cache.len(),
            1,
            "a full cache starts over with the new entry"
        );
        assert_eq!(cache.get(&CACHE_LIMIT), Some(&CACHE_LIMIT));
    }

    #[test]
    fn a_changed_block_keeps_its_last_picture_while_it_renders() {
        let last = Some("<svg old/>".to_owned());
        let ready = Typeset::Svg("<svg new/>".into());
        assert_eq!(shown(Some(&ready), last.as_ref()), Shown::Svg("<svg new/>"));
        assert_eq!(
            shown(Some(&Typeset::Pending), last.as_ref()),
            Shown::Svg("<svg old/>")
        );
        assert_eq!(shown(None, last.as_ref()), Shown::Svg("<svg old/>"));
        assert_eq!(shown(Some(&Typeset::Pending), None), Shown::Pending);
        let error = Typeset::Error("bad".into());
        assert_eq!(
            shown(Some(&error), last.as_ref()),
            Shown::Error("bad"),
            "an error is current"
        );
    }

    #[test]
    fn a_failed_download_is_retried_after_a_while() {
        let failed = Instant::now();
        assert!(!retry_due(failed, failed + Duration::from_secs(5)));
        assert!(retry_due(failed, failed + Duration::from_secs(61)));
    }
}
