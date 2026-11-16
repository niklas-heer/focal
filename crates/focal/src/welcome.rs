//! The Welcome document: what the first launch shows, and what Help ▸ Focal
//! Help opens. It is `examples/welcome.md`, compiled into the app, so it
//! works offline and needs no file beside the app.

use gpui_kit::App;

use crate::instance::Request;
use crate::windows;

pub const TEXT: &str = include_str!("../../../examples/welcome.md");

/// A request for a new untitled document holding the Welcome text.
pub fn request() -> Request {
    Request {
        untitled: Some(TEXT.to_owned()),
        ..Request::default()
    }
}

/// Opens the Welcome document in a new window.
pub fn open(cx: &mut App) {
    windows::open(request(), None, cx);
}

#[cfg(test)]
mod tests {
    use focal_core::analysis::{LineKind, analyze};
    use focal_core::blocks::{DiagramLanguage, diagram_blocks, math_blocks};

    use super::TEXT;

    #[test]
    fn the_welcome_document_parses_into_its_blocks() {
        let analysis = analyze(TEXT);
        let headings: Vec<&str> = (0..analysis.line_count())
            .filter(|&line| matches!(analysis.info(line).kind, LineKind::Heading(_)))
            .map(|line| &TEXT[analysis.lines.range(line)])
            .collect();
        assert_eq!(
            headings,
            [
                "# Welcome to Focal",
                "## Try it",
                "## Shortcuts for the basics",
                "## A formula",
                "## A diagram",
                "## Getting around",
            ]
        );
        assert_eq!(analysis.tables.len(), 1, "one small table");
        assert_eq!(math_blocks(&analysis, TEXT).len(), 1, "one formula");
        let diagrams = diagram_blocks(&analysis, TEXT);
        assert_eq!(diagrams.len(), 1, "one diagram");
        assert_eq!(diagrams[0].language, DiagramLanguage::Mermaid);
        assert_eq!(TEXT.matches("- [ ]").count(), 3, "three boxes to tick");
    }
}
