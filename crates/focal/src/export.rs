//! Export as HTML… and Copy as HTML: the document rendered the way the
//! editor shows it, with typeset math and Mermaid diagrams as SVG.

use std::fmt::Write as _;
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
        Embed::Diagram(language, source) => diagram::render(language, source, &palette).ok(),
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
        Embed::Diagram(language, source) => diagram::render(language, source, &palette)
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

/// Writes `text` as a page for paper into the folder `out`, beside the
/// typefaces it is set in, and returns the page's path. Printing and PDF
/// export render this page; its relative sources lead from the
/// document's folder.
pub fn write_print_page(
    text: &str,
    title: &str,
    sources: &Sources,
    out: &Path,
) -> anyhow::Result<PathBuf> {
    std::fs::create_dir_all(out)?;
    let mut faces = String::new();
    for font in &crate::theme::FONTS {
        let file = out.join(font.file);
        std::fs::write(&file, font.data)?;
        let mut families = vec![font.family];
        // Quattro has no bold of its own; the editor sets it in Duo's, too.
        if font.bold && font.family == "iA Writer Duo S" {
            families.push("iA Writer Quattro S");
        }
        for family in families {
            let _ = writeln!(
                faces,
                "@font-face {{ font-family: \"{family}\"; src: url(\"file://{}\"); font-weight: {}; font-style: {}; }}",
                file.display(),
                if font.bold { "bold" } else { "normal" },
                if font.italic { "italic" } else { "normal" },
            );
        }
    }
    let folder = sources.folder().unwrap_or(out);
    let body = html(text, sources, folder);
    let base = format!("file://{}/", folder.display());
    let page = out.join("page.html");
    std::fs::write(&page, export::print_page(title, &body, &faces, Some(&base)))?;
    Ok(page)
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
    let beside = folder.join(source);
    if destination == Some(folder) && beside.exists() {
        return None;
    }
    // Obsidian embeds `![[pic.png]]` may live anywhere in the folder.
    let file = if beside.exists() {
        beside
    } else {
        links::find_file(folder, &links::percent_decode(source), 20_000).unwrap_or(beside)
    };
    Some(match destination.and_then(|d| file.strip_prefix(d).ok()) {
        Some(relative) => relative.display().to_string(),
        None => format!("file://{}", file.display()),
    })
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
    fn a_print_folder_holds_the_page_and_its_typefaces() {
        let folder = temp("print-document");
        let out = temp("print");
        let page = write_print_page(
            "# Cats\n\n![a cat](cat.png)\n",
            "Cats",
            &sources(&folder),
            &out,
        )
        .unwrap();
        let html = std::fs::read_to_string(&page).unwrap();
        assert!(html.contains("@page"), "a page for paper");
        let base = format!(r#"<base href="file://{}/">"#, folder.display());
        assert!(
            html.contains(&base),
            "images lead from the document: {html}"
        );
        assert!(html.contains(r#"src="cat.png""#), "{html}");
        for font in &crate::theme::FONTS {
            let file = out.join(font.file);
            assert!(file.is_file(), "{} is beside the page", font.file);
            let url = format!("url(\"file://{}\")", file.display());
            assert!(html.contains(&url), "{html}");
        }
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
