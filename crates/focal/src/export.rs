//! Export as HTML… and Copy as HTML: the document rendered the way the
//! editor shows it, with typeset math and Mermaid diagrams as SVG.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use focal_core::export::{self, Embed};
use focal_core::links;
use gpui_kit::WindowAppearance;

use crate::theme::Theme;
use crate::{diagram, islands, math};

/// Where a document's links and images lead from.
#[derive(Clone, Default)]
pub struct Sources {
    /// The document's file.
    pub document: Option<PathBuf>,
    /// The folder wiki links resolve in, and its Markdown files relative to it.
    pub link_root: Option<PathBuf>,
    pub link_files: Arc<[PathBuf]>,
}

impl Sources {
    /// The file a wiki link to `destination` opens, if it exists.
    pub fn wiki_file(&self, destination: &str) -> Option<PathBuf> {
        let root = self.link_root.as_ref()?;
        let (name, _) = links::wiki_target(destination);
        let document = self
            .document
            .as_deref()
            .and_then(|path| path.strip_prefix(root).ok())
            .unwrap_or(Path::new(""));
        links::resolve_wiki(&name, document, &self.link_files).map(|file| root.join(file))
    }

    fn folder(&self) -> Option<&Path> {
        self.document.as_deref().and_then(Path::parent)
    }
}

/// `text` as an HTML fragment for a page in the folder `destination`, with
/// math and diagrams as SVG. Typesets and draws, so run it in the
/// background.
pub fn html(text: &str, sources: &Sources, destination: &Path) -> String {
    let palette = light_palette();
    export::to_html(text, &mut |embed| match embed {
        Embed::DisplayMath(tex) => math::typeset_as(tex, true).ok(),
        Embed::InlineMath(tex) => math::typeset_as(tex, false).ok(),
        Embed::Diagram(source) => diagram::render(source, &palette).ok(),
        Embed::WikiLink(name) => sources
            .wiki_file(name)
            .map(|file| reference(&file, Some(destination))),
        Embed::Image(source) => image_source(source, sources.folder(), Some(destination)),
    })
}

/// `text` as HTML for the pasteboard: local links and images as absolute
/// `file://` URLs, and math and diagrams as PNG pictures, which mail and
/// word processors show where they often drop SVG.
pub fn pasteboard_html(text: &str, sources: &Sources) -> String {
    let palette = light_palette();
    export::to_html(text, &mut |embed| match embed {
        Embed::DisplayMath(tex) => math_picture(tex, true),
        Embed::InlineMath(tex) => math_picture(tex, false),
        Embed::Diagram(source) => diagram::render(source, &palette)
            .ok()
            .and_then(|svg| diagram_picture(&svg)),
        Embed::WikiLink(name) => sources.wiki_file(name).map(|file| reference(&file, None)),
        Embed::Image(source) => image_source(source, sources.folder(), None),
    })
}

fn light_palette() -> diagram::Palette {
    islands::palette(&Theme::for_appearance(WindowAppearance::Light))
}

/// The text size pasted math is drawn for.
const PASTED_TEXT_SIZE: f32 = 16.;

fn math_picture(tex: &str, display: bool) -> Option<String> {
    let svg = math::typeset_as(tex, display).ok()?;
    let sized = math::sized(&svg, "#212121", PASTED_TEXT_SIZE, 2.)?;
    let style = if display {
        ""
    } else {
        "vertical-align: middle"
    };
    picture(&sized.svg, sized.width, sized.height, style)
}

fn diagram_picture(svg: &str) -> Option<String> {
    let (width, height) = diagram::svg_size(svg)?;
    let sharp = diagram::with_size(svg, width * 2., height * 2.);
    picture(sharp.as_bytes(), width, height, "")
}

/// An `<img>` showing `svg` as a PNG at twice `width` × `height`.
fn picture(svg: &[u8], width: f32, height: f32, style: &str) -> Option<String> {
    use base64::Engine as _;
    let png = rasterize(svg)?;
    Some(format!(
        r#"<img src="data:image/png;base64,{}" width="{}" height="{}" style="{style}" alt="">"#,
        base64::engine::general_purpose::STANDARD.encode(png),
        width.round(),
        height.round()
    ))
}

fn rasterize(svg: &[u8]) -> Option<Vec<u8>> {
    use resvg::{tiny_skia, usvg};
    static FONTS: std::sync::LazyLock<Arc<usvg::fontdb::Database>> =
        std::sync::LazyLock::new(|| {
            let mut fonts = usvg::fontdb::Database::new();
            fonts.load_system_fonts();
            Arc::new(fonts)
        });
    let options = usvg::Options {
        fontdb: FONTS.clone(),
        ..usvg::Options::default()
    };
    let tree = usvg::Tree::from_data(svg, &options).ok()?;
    let size = tree.size().to_int_size();
    let mut pixmap = tiny_skia::Pixmap::new(size.width(), size.height())?;
    resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());
    pixmap.encode_png().ok()
}

/// Writes `text` as a standalone page titled `title` to `path`.
pub fn write_page(text: &str, title: &str, sources: &Sources, path: &Path) -> anyhow::Result<()> {
    let body = html(text, sources, path.parent().unwrap_or(Path::new("/")));
    std::fs::write(path, export::page(title, &body))?;
    Ok(())
}

/// How a page in `destination` refers to `file`: relative when the file is
/// in or below that folder, otherwise by its `file://` URL.
fn reference(file: &Path, destination: Option<&Path>) -> String {
    destination
        .and_then(|folder| file.strip_prefix(folder).ok())
        .map_or_else(
            || format!("file://{}", file.display()),
            |relative| relative.display().to_string(),
        )
}

/// A relative image source for a page in `destination`: unchanged beside
/// the document, otherwise the image's `file://` URL. Remote and absolute
/// sources stay as they are.
fn image_source(source: &str, folder: Option<&Path>, destination: Option<&Path>) -> Option<String> {
    let folder = folder?;
    if source.contains("://") || source.starts_with('/') || source.starts_with("data:") {
        return None;
    }
    if destination == Some(folder) {
        return None;
    }
    Some(format!("file://{}", folder.join(source).display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("focal-export-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sources(folder: &Path) -> Sources {
        Sources {
            document: Some(folder.join("note.md")),
            ..Sources::default()
        }
    }

    #[test]
    fn a_page_carries_typeset_math_and_drawn_diagrams() {
        let folder = temp("page");
        let page = folder.join("note.html");
        let text = "# Note\n\nInline $x^2$.\n\n$$\ny = 1\n$$\n\n```mermaid\nflowchart TD\n  A --> B\n```\n";
        write_page(text, "Note", &sources(&folder), &page).unwrap();
        let html = std::fs::read_to_string(&page).unwrap();
        assert!(html.contains("<title>Note</title>"));
        assert!(
            html.contains(r#"<span class="math"><svg"#),
            "inline math is typeset"
        );
        assert!(
            html.contains(r#"<div class="math"><svg"#),
            "display math is typeset"
        );
        assert!(
            html.contains(r#"<figure class="diagram"><svg"#),
            "the diagram is drawn"
        );
    }

    #[test]
    fn images_keep_relative_sources_beside_the_document_only() {
        let folder = temp("images");
        let elsewhere = temp("images-elsewhere");
        let text = "![a](pics/a.png)\n\n![b](https://example.com/b.png)\n";
        let beside = html(text, &sources(&folder), &folder);
        assert!(beside.contains(r#"src="pics/a.png""#), "{beside}");
        let moved = html(text, &sources(&folder), &elsewhere);
        let absolute = format!(r#"src="file://{}/pics/a.png""#, folder.display());
        assert!(moved.contains(&absolute), "{moved}");
        assert!(
            moved.contains(r#"src="https://example.com/b.png""#),
            "{moved}"
        );
        let copied = pasteboard_html(text, &sources(&folder));
        assert!(
            copied.contains(&absolute),
            "the pasteboard gets absolute sources"
        );
    }

    #[test]
    fn wiki_links_lead_to_the_files_they_open() {
        let folder = temp("wiki");
        std::fs::write(folder.join("other.md"), "").unwrap();
        let sources = Sources {
            document: Some(folder.join("note.md")),
            link_root: Some(folder.clone()),
            link_files: Arc::from(vec![PathBuf::from("other.md"), PathBuf::from("note.md")]),
        };
        let page = html("See [[Other]] and [[Missing]].\n", &sources, &folder);
        assert!(page.contains(r#"href="other.md""#), "{page}");
        assert!(page.contains(r#"href="Missing.md""#), "{page}");
    }

    #[test]
    fn the_pasteboard_gets_math_and_diagrams_as_pictures() {
        let folder = temp("pasteboard");
        let text = "Inline $x^2$.\n\n```mermaid\nflowchart TD\n  A --> B\n```\n";
        let html = pasteboard_html(text, &sources(&folder));
        assert!(
            !html.contains("<svg"),
            "mail clients rarely show SVG: {html}"
        );
        assert_eq!(
            html.matches(r#"src="data:image/png;base64,"#).count(),
            2,
            "{html}"
        );
    }
}
