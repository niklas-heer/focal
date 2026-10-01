//! Where links lead: wiki links resolved against a folder's files, relative
//! Markdown links against the document, and `#heading` anchors.

use std::path::{Path, PathBuf};

use crate::analysis::{Analysis, LineKind};

/// Splits the inside of `[[…]]` into the target name and the label after
/// `|`; a `#heading` part is dropped.
pub fn wiki_target(inner: &str) -> (String, Option<String>) {
    let (target, label) = match inner.split_once('|') {
        Some((target, label)) => (target, Some(label.trim().to_owned())),
        None => (inner, None),
    };
    let target = target.split_once('#').map_or(target, |(name, _)| name);
    (target.trim().to_owned(), label)
}

/// The file in `files` (paths relative to the folder) that a wiki link names,
/// matching the file name with or without its extension, or a relative path,
/// without regard to case. Files in the document's own folder come first,
/// then shorter paths, then alphabetical order.
pub fn resolve_wiki(name: &str, document: &Path, files: &[PathBuf]) -> Option<PathBuf> {
    let wanted = name.to_lowercase();
    let without_extension = |path: &Path| path.with_extension("").to_string_lossy().to_lowercase();
    let here = document.parent().unwrap_or(Path::new(""));
    files
        .iter()
        .filter(|file| {
            let path = file.to_string_lossy().to_lowercase();
            let stem = file
                .file_stem()
                .map(|s| s.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            let file_name = file
                .file_name()
                .map(|s| s.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            if wanted.contains('/') {
                path == wanted || without_extension(file) == wanted
            } else {
                stem == wanted || file_name == wanted
            }
        })
        .min_by_key(|file| {
            let elsewhere = file.parent().unwrap_or(Path::new("")) != here;
            (
                elsewhere,
                file.components().count(),
                file.to_string_lossy().to_lowercase(),
            )
        })
        .cloned()
}

/// Decodes `%XX` escapes; invalid ones stay as they are.
pub fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(byte) = text
                .get(i + 1..i + 3)
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
        {
            decoded.push(byte);
            i += 3;
        } else {
            decoded.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

/// The local file a link's destination points to, relative to the
/// document's folder. Web and mail links and same-document anchors are not
/// files.
pub fn relative_target(destination: &str, document_dir: &Path) -> Option<PathBuf> {
    if destination.contains("://")
        || destination.starts_with("mailto:")
        || destination.starts_with('#')
    {
        return None;
    }
    let path = destination.split(['#', '?']).next().unwrap_or_default();
    if path.is_empty() {
        return None;
    }
    let joined = document_dir.join(percent_decode(path));
    // Resolve `..` without touching the file system.
    let mut normal = PathBuf::new();
    for component in joined.components() {
        match component {
            std::path::Component::ParentDir => {
                normal.pop();
            }
            std::path::Component::CurDir => {}
            other => normal.push(other),
        }
    }
    Some(normal)
}

/// GitHub's anchor for a heading: lowercase, punctuation removed, spaces as
/// hyphens.
pub fn heading_slug(heading: &str) -> String {
    heading
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_'))
        .map(|c| if c == ' ' { '-' } else { c })
        .collect()
}

/// Where the text of the heading with anchor `slug` starts.
pub fn find_heading(analysis: &Analysis, text: &str, slug: &str) -> Option<usize> {
    (0..analysis.line_count()).find_map(|line| {
        if !matches!(analysis.info(line).kind, LineKind::Heading(_)) {
            return None;
        }
        let range = analysis.lines.range(line);
        let source = &text[range.clone()];
        let lead = source.len() - source.trim_start_matches(['#', ' ']).len();
        let title = source[lead..].trim_end_matches(['#', ' ']);
        (heading_slug(title) == slug).then_some(range.start + lead)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::analyze;

    fn paths(list: &[&str]) -> Vec<PathBuf> {
        list.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn wiki_targets_split_label_and_heading() {
        assert_eq!(wiki_target("Design"), ("Design".into(), None));
        assert_eq!(
            wiki_target("Design|the design"),
            ("Design".into(), Some("the design".into()))
        );
        assert_eq!(
            wiki_target("notes/Design#Goals"),
            ("notes/Design".into(), None)
        );
    }

    #[test]
    fn wiki_links_resolve_case_insensitively_nearest_first() {
        let files = paths(&[
            "design.md",
            "notes/design.md",
            "notes/deep/Design.markdown",
            "other.md",
        ]);
        assert_eq!(
            resolve_wiki("Design", Path::new("notes/today.md"), &files),
            Some("notes/design.md".into()),
            "the same folder wins"
        );
        assert_eq!(
            resolve_wiki("DESIGN", Path::new("index.md"), &files),
            Some("design.md".into())
        );
        assert_eq!(
            resolve_wiki("Design", Path::new("elsewhere/x.md"), &files),
            Some("design.md".into()),
            "then the shortest path"
        );
        assert_eq!(
            resolve_wiki("notes/deep/design", Path::new("index.md"), &files),
            Some("notes/deep/Design.markdown".into())
        );
        assert_eq!(
            resolve_wiki("other.md", Path::new("index.md"), &files),
            Some("other.md".into())
        );
        assert_eq!(resolve_wiki("missing", Path::new("index.md"), &files), None);
    }

    #[test]
    fn relative_links_resolve_against_the_document() {
        let dir = Path::new("/notes/today");
        assert_eq!(
            relative_target("../My%20Notes.md#top", dir),
            Some("/notes/My Notes.md".into())
        );
        assert_eq!(
            relative_target("plan.md", dir),
            Some("/notes/today/plan.md".into())
        );
        assert_eq!(relative_target("https://example.com/a.md", dir), None);
        assert_eq!(relative_target("mailto:a@b.c", dir), None);
        assert_eq!(relative_target("#goals", dir), None);
    }

    #[test]
    fn headings_are_found_by_their_slug() {
        assert_eq!(
            heading_slug("1. Goal and principles (Agreed)"),
            "1-goal-and-principles-agreed"
        );
        assert_eq!(heading_slug("Café & Co"), "café--co");
        let text = "# Title\n\nbody\n\n## Next Steps\n";
        let analysis = analyze(text);
        assert_eq!(
            find_heading(&analysis, text, "next-steps"),
            Some(18),
            "the heading text"
        );
        assert_eq!(find_heading(&analysis, text, "missing"), None);
    }
}
