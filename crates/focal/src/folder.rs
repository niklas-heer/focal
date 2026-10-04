//! The sidebar's file list: the Markdown files under a folder (folder mode)
//! or beside a single file, kept current through FSEvents.

use std::path::{Path, PathBuf};

/// Stop scanning after this many files, so a huge tree cannot freeze Focal.
pub const SCAN_LIMIT: usize = 5_000;

const EXTENSIONS: [&str; 4] = ["md", "markdown", "mdown", "mkd"];
/// Folders that hold dependencies or build output, not notes.
const SKIPPED: [&str; 2] = ["node_modules", "target"];

/// How far below its root a folder is listed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Depth {
    /// Every subfolder: a folder opened in folder mode.
    Tree,
    /// The root alone: the folder around a single file.
    Level,
}

/// The Markdown files under `root`, as paths relative to it, sorted without
/// regard to case. Hidden entries and dependency or build folders are skipped;
/// scanning stops after `limit` files.
pub fn scan(root: &Path, depth: Depth, limit: usize) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut folders = vec![root.to_path_buf()];
    while let Some(folder) = folders.pop() {
        let Ok(entries) = std::fs::read_dir(&folder) else {
            continue;
        };
        let mut entries: Vec<_> = entries.flatten().collect();
        entries.sort_by_key(|entry| entry.file_name().to_ascii_lowercase());
        // Visit subfolders in order: the stack takes the last one first.
        for entry in entries.iter().rev() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.') {
                continue;
            }
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if depth == Depth::Tree && kind.is_dir() && !SKIPPED.contains(&name.as_ref()) {
                folders.push(entry.path());
            }
        }
        for entry in &entries {
            let path = entry.path();
            let name = entry.file_name();
            let markdown = path
                .extension()
                .is_some_and(|e| EXTENSIONS.iter().any(|x| e.eq_ignore_ascii_case(x)));
            // Review notes belong to their document; the Notes tab opens them.
            if markdown
                && !focal_core::review::is_sidecar(&path)
                && !name.to_string_lossy().starts_with('.')
                && entry.file_type().is_ok_and(|t| t.is_file())
                && let Ok(relative) = path.strip_prefix(root)
            {
                files.push(relative.to_path_buf());
                if files.len() >= limit {
                    folders.clear();
                    break;
                }
            }
        }
    }
    files.sort_by_key(|path| path.to_string_lossy().to_lowercase());
    files
}

/// Files larger than this are not read for their tags.
const TAG_FILE_LIMIT: u64 = 2_000_000;

/// The tags of `files` (relative to `root`): each tag with the files that
/// carry it, by name without regard to case; a nested tag's parents are
/// listed with the files of the tags under them.
pub fn tags(root: &Path, files: &[PathBuf]) -> Vec<(String, Vec<PathBuf>)> {
    let mut by_tag: std::collections::BTreeMap<String, Vec<PathBuf>> =
        std::collections::BTreeMap::new();
    for file in files {
        let path = root.join(file);
        if std::fs::metadata(&path).is_ok_and(|m| m.len() > TAG_FILE_LIMIT) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        for tag in focal_core::tags::document_tags(&text) {
            // A nested tag counts for its parents too: `a/b` lists under `a`.
            let mut parent = tag.as_str();
            while let Some((up, _)) = parent.rsplit_once('/') {
                parent = up;
                let files = by_tag.entry(up.to_owned()).or_default();
                if files.last() != Some(file) {
                    files.push(file.clone());
                }
            }
            by_tag.entry(tag).or_default().push(file.clone());
        }
    }
    let mut tags: Vec<_> = by_tag.into_iter().collect();
    tags.sort_by_cached_key(|(tag, _)| tag.to_lowercase());
    tags
}

/// A link to a document from another file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Backlink {
    /// The linking file, relative to the folder's root.
    pub file: PathBuf,
    /// Where the link starts in that file.
    pub offset: usize,
    /// The line holding the link, trimmed and shortened.
    pub line: String,
    /// The link within `line`.
    pub label: std::ops::Range<usize>,
}

/// How much of a linking line the panel shows.
const BACKLINK_LINE: usize = 120;

/// Every link between `files` (relative to `root`), by the file it leads
/// to: wiki links and relative Markdown links, each counted once per link,
/// a file's links to itself left out. Review notes are not documents.
pub fn backlinks(
    root: &Path,
    files: &[PathBuf],
) -> std::collections::HashMap<PathBuf, Vec<Backlink>> {
    use focal_core::links::{self, LinkTarget};
    let mut index: std::collections::HashMap<PathBuf, Vec<Backlink>> =
        std::collections::HashMap::new();
    for file in files {
        let path = root.join(file);
        if std::fs::metadata(&path).is_ok_and(|m| m.len() > TAG_FILE_LIMIT) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        for link in links::outgoing_links(&text) {
            let target = match &link.target {
                LinkTarget::Wiki(name) => links::resolve_wiki(name, file, files),
                LinkTarget::Path(destination) => {
                    let dir = path.parent().unwrap_or(root);
                    links::relative_target(destination, dir)
                        .and_then(|target| target.strip_prefix(root).ok().map(Path::to_path_buf))
                        .and_then(|target| {
                            files
                                .iter()
                                .find(|file| {
                                    file.to_string_lossy().to_lowercase()
                                        == target.to_string_lossy().to_lowercase()
                                })
                                .cloned()
                        })
                }
            };
            let Some(target) = target.filter(|target| target != file) else {
                continue;
            };
            index
                .entry(target)
                .or_default()
                .push(backlink(file, &text, link.range));
        }
    }
    index
}

/// The backlink for `range` in `file`'s `text`: its line, trimmed and cut
/// to [`BACKLINK_LINE`] characters around the link.
fn backlink(file: &Path, text: &str, range: std::ops::Range<usize>) -> Backlink {
    let start = text[..range.start].rfind('\n').map_or(0, |at| at + 1);
    let end = text[range.start..]
        .find('\n')
        .map_or(text.len(), |at| range.start + at);
    let raw = &text[start..end];
    let trimmed_start = raw.len() - raw.trim_start().len();
    let line = raw.trim();
    let label_start = range.start - start - trimmed_start;
    let label_end = (range.end - start - trimmed_start).min(line.len());
    // Keep the link in view: cut before it when the line is long.
    let cut = line
        .char_indices()
        .map(|(ix, _)| ix)
        .find(|&ix| label_start.saturating_sub(ix) < BACKLINK_LINE / 2)
        .unwrap_or(0);
    let tail = &line[cut..];
    let kept = tail
        .char_indices()
        .nth(BACKLINK_LINE)
        .map_or(tail.len(), |(ix, _)| ix);
    let (prefix, suffix) = (
        if cut > 0 { "…" } else { "" },
        if kept < tail.len() { "…" } else { "" },
    );
    let shown = format!("{prefix}{}{suffix}", &tail[..kept]);
    let shift = prefix.len();
    Backlink {
        file: file.to_path_buf(),
        offset: range.start,
        label: (label_start - cut + shift).min(shown.len())
            ..(label_end - cut + shift).min(shown.len()),
        line: shown,
    }
}

/// The most recently modified of `files` (relative to `root`).
pub fn newest(root: &Path, files: &[PathBuf]) -> Option<PathBuf> {
    files
        .iter()
        .max_by_key(|path| {
            std::fs::metadata(root.join(path))
                .and_then(|m| m.modified())
                .ok()
        })
        .cloned()
}

/// Watches `root`, and with [`Depth::Tree`] everything under it; the
/// receiver gets a message per batch of changes.
pub fn watch(
    root: &Path,
    depth: Depth,
) -> anyhow::Result<(notify::RecommendedWatcher, async_channel::Receiver<()>)> {
    use notify::{RecursiveMode, Watcher as _};
    let (tx, rx) = async_channel::unbounded();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        if event.is_ok() {
            let _ = tx.try_send(());
        }
    })?;
    let mode = match depth {
        Depth::Tree => RecursiveMode::Recursive,
        Depth::Level => RecursiveMode::NonRecursive,
    };
    watcher.watch(root, mode)?;
    Ok((watcher, rx))
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn review_sidecars_are_not_listed() {
        let root = temp_folder("sidecars");
        std::fs::write(root.join("a.md"), "").unwrap();
        std::fs::write(root.join("a.review.md"), "").unwrap();
        assert_eq!(
            scan(&root, Depth::Tree, SCAN_LIMIT),
            [PathBuf::from("a.md")]
        );
    }

    #[test]
    fn link_spellings_resolve_and_self_is_skipped() {
        let root = temp_folder("backlinks");
        std::fs::write(root.join("Name Two.md"), "[[Name Two]] links itself").unwrap();
        std::fs::write(root.join("a.md"), "[[Name Two]] and [[name two.md]]").unwrap();
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::write(root.join("sub/b.md"), "[x](../Name%20Two.md#part)").unwrap();
        std::fs::write(root.join("a.review.md"), "[[Name Two]]").unwrap();
        let files = scan(&root, Depth::Tree, SCAN_LIMIT);
        let index = backlinks(&root, &files);
        let from: Vec<_> = index[Path::new("Name Two.md")]
            .iter()
            .map(|b| b.file.clone())
            .collect();
        assert_eq!(
            from,
            [
                PathBuf::from("a.md"),
                PathBuf::from("a.md"),
                PathBuf::from("sub/b.md")
            ]
        );
        let first = &index[Path::new("Name Two.md")][0];
        assert_eq!(first.line, "[[Name Two]] and [[name two.md]]");
        assert_eq!(&first.line[first.label.clone()], "[[Name Two]]");
    }

    /// A fresh folder under the system's temporary folder.
    pub fn temp_folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("focal-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn touch(root: &Path, path: &str) {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "x").unwrap();
    }

    #[test]
    fn scans_markdown_files_sorted_and_skips_hidden_and_build_folders() {
        let root = temp_folder("scan");
        for path in [
            "b.md",
            "A/c.markdown",
            "a.MD",
            ".hidden/x.md",
            ".dot.md",
            "node_modules/y.md",
            "target/z.md",
            "notes.txt",
        ] {
            touch(&root, path);
        }
        let files = scan(&root, Depth::Tree, SCAN_LIMIT);
        let files: Vec<_> = files
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        assert_eq!(files, ["a.MD", "A/c.markdown", "b.md"]);
    }

    #[test]
    fn tags_list_the_files_that_carry_them() {
        let root = temp_folder("tags");
        std::fs::write(root.join("a.md"), "An #idea and #work/focal").unwrap();
        std::fs::write(root.join("b.md"), "---\ntags: [idea]\n---\nText #work/home").unwrap();
        let files = scan(&root, Depth::Tree, SCAN_LIMIT);
        let tags = tags(&root, &files);
        let names: Vec<_> = tags
            .iter()
            .map(|(tag, files)| (tag.as_str(), files.len()))
            .collect();
        assert_eq!(
            names,
            [
                ("idea", 2),
                ("work", 2),
                ("work/focal", 1),
                ("work/home", 1)
            ],
            "parents are listed with their children's files"
        );
    }

    #[test]
    fn scanning_stops_at_the_limit() {
        let root = temp_folder("limit");
        for i in 0..10 {
            touch(&root, &format!("{i}.md"));
        }
        assert_eq!(scan(&root, Depth::Tree, 4).len(), 4);
    }

    #[test]
    fn a_level_lists_only_the_files_beside_each_other() {
        let root = temp_folder("level");
        for path in ["b.md", "a.markdown", "sub/c.md", "notes.txt"] {
            touch(&root, path);
        }
        let files = scan(&root, Depth::Level, SCAN_LIMIT);
        let files: Vec<_> = files
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        assert_eq!(files, ["a.markdown", "b.md"]);
    }
}
