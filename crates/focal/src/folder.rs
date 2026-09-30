//! Folder mode's file list: the Markdown files under a folder, kept current
//! through FSEvents.

use std::path::{Path, PathBuf};

/// Stop scanning after this many files, so a huge tree cannot freeze Focal.
pub const SCAN_LIMIT: usize = 5_000;

const EXTENSIONS: [&str; 4] = ["md", "markdown", "mdown", "mkd"];
/// Folders that hold dependencies or build output, not notes.
const SKIPPED: [&str; 2] = ["node_modules", "target"];

/// The Markdown files under `root`, as paths relative to it, sorted without
/// regard to case. Hidden entries and dependency or build folders are skipped;
/// scanning stops after `limit` files.
pub fn scan(root: &Path, limit: usize) -> Vec<PathBuf> {
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
            if kind.is_dir() && !SKIPPED.contains(&name.as_ref()) {
                folders.push(entry.path());
            }
        }
        for entry in &entries {
            let path = entry.path();
            let name = entry.file_name();
            let markdown = path
                .extension()
                .is_some_and(|e| EXTENSIONS.iter().any(|x| e.eq_ignore_ascii_case(x)));
            if markdown
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

/// Watches `root` and everything under it; the receiver gets a message per
/// batch of changes.
pub fn watch(
    root: &Path,
) -> anyhow::Result<(notify::RecommendedWatcher, async_channel::Receiver<()>)> {
    use notify::{RecursiveMode, Watcher as _};
    let (tx, rx) = async_channel::unbounded();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        if event.is_ok() {
            let _ = tx.try_send(());
        }
    })?;
    watcher.watch(root, RecursiveMode::Recursive)?;
    Ok((watcher, rx))
}

#[cfg(test)]
pub mod tests {
    use super::*;

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
        let files = scan(&root, SCAN_LIMIT);
        let files: Vec<_> = files
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        assert_eq!(files, ["a.MD", "A/c.markdown", "b.md"]);
    }

    #[test]
    fn scanning_stops_at_the_limit() {
        let root = temp_folder("limit");
        for i in 0..10 {
            touch(&root, &format!("{i}.md"));
        }
        assert_eq!(scan(&root, 4).len(), 4);
    }
}
