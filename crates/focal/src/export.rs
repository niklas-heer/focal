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

/// `text` as an HTML fragment, for a page in the folder `destination`, or
/// for the pasteboard with `None` (local links and images then point to
/// absolute `file://` URLs). Typesets math and draws diagrams, so run it in
/// the background.
pub fn html(text: &str, sources: &Sources, destination: Option<&Path>) -> String {
    let palette = islands::palette(&Theme::for_appearance(WindowAppearance::Light));
    export::to_html(text, &mut |embed| match embed {
        Embed::DisplayMath(tex) => math::typeset_as(tex, true).ok(),
        Embed::InlineMath(tex) => math::typeset_as(tex, false).ok(),
        Embed::Diagram(source) => diagram::render(source, &palette).ok(),
        Embed::WikiLink(name) => sources
            .wiki_file(name)
            .map(|file| reference(&file, destination)),
        Embed::Image(source) => image_source(source, sources.folder(), destination),
    })
}

/// Writes `text` as a standalone page titled `title` to `path`.
pub fn write_page(text: &str, title: &str, sources: &Sources, path: &Path) -> anyhow::Result<()> {
    let body = html(text, sources, path.parent());
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
        let beside = html(text, &sources(&folder), Some(&folder));
        assert!(beside.contains(r#"src="pics/a.png""#), "{beside}");
        let moved = html(text, &sources(&folder), Some(&elsewhere));
        let absolute = format!(r#"src="file://{}/pics/a.png""#, folder.display());
        assert!(moved.contains(&absolute), "{moved}");
        assert!(
            moved.contains(r#"src="https://example.com/b.png""#),
            "{moved}"
        );
        let copied = html(text, &sources(&folder), None);
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
        let page = html("See [[Other]] and [[Missing]].\n", &sources, Some(&folder));
        assert!(page.contains(r#"href="other.md""#), "{page}");
        assert!(page.contains(r#"href="Missing.md""#), "{page}");
    }
}
