//! Folds opened or closed by hand, remembered per file in
//! `~/Library/Application Support/Focal/folds.json`, so a document opens the
//! way it was left.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use gpui_kit::{App, Global};

/// Files whose folds are kept; the oldest give way beyond this.
const FILE_LIMIT: usize = 500;

#[derive(Default)]
pub struct FoldMemory {
    /// By file path, each fold's state by its key, and when it was last set.
    files: BTreeMap<String, (u64, HashMap<String, bool>)>,
    /// Where the memory is kept; `None` keeps it in memory only (tests).
    file: Option<PathBuf>,
}

impl Global for FoldMemory {}

impl FoldMemory {
    /// The memory kept in `file`; empty when it is missing or unreadable.
    pub fn load_from(file: &Path) -> Self {
        let files = std::fs::read_to_string(file)
            .ok()
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default();
        Self {
            files,
            file: Some(file.to_owned()),
        }
    }

    /// The folds remembered for `path`.
    pub fn get(&self, path: &Path) -> HashMap<String, bool> {
        self.files
            .get(&path.to_string_lossy().into_owned())
            .map(|(_, folds)| folds.clone())
            .unwrap_or_default()
    }

    /// Remembers that the fold `key` in `path` is `folded`, and saves.
    pub fn set(&mut self, path: &Path, key: &str, folded: bool) {
        let stamp = self
            .files
            .values()
            .map(|(stamp, _)| *stamp)
            .max()
            .unwrap_or(0)
            + 1;
        let entry = self
            .files
            .entry(path.to_string_lossy().into_owned())
            .or_default();
        entry.0 = stamp;
        entry.1.insert(key.to_owned(), folded);
        while self.files.len() > FILE_LIMIT {
            let oldest = self
                .files
                .iter()
                .min_by_key(|(_, (stamp, _))| *stamp)
                .map(|(path, _)| path.clone());
            if let Some(oldest) = oldest {
                self.files.remove(&oldest);
            }
        }
        if let Some(file) = &self.file
            && let Err(error) = save(file, &self.files)
        {
            eprintln!("focal: remembering folds: {error:#}");
        }
    }
}

fn save(file: &Path, files: &BTreeMap<String, (u64, HashMap<String, bool>)>) -> anyhow::Result<()> {
    if let Some(folder) = file.parent() {
        std::fs::create_dir_all(folder)?;
    }
    std::fs::write(file, serde_json::to_string(files)?)?;
    Ok(())
}

/// Loads the memory from Application Support.
pub fn init(cx: &mut App) {
    if let Some(home) = std::env::var_os("HOME") {
        let file = PathBuf::from(home).join("Library/Application Support/Focal/folds.json");
        cx.set_global(FoldMemory::load_from(&file));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_are_remembered_per_file_across_launches() {
        let dir = std::env::temp_dir().join(format!("focal-folds-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let file = dir.join("folds.json");
        let mut memory = FoldMemory::load_from(&file);
        memory.set(Path::new("/notes/a.md"), "> [!faq]- Why", false);
        memory.set(Path::new("/notes/b.md"), "<summary>More</summary>", true);
        let again = FoldMemory::load_from(&file);
        assert_eq!(
            again.get(Path::new("/notes/a.md")).get("> [!faq]- Why"),
            Some(&false)
        );
        assert_eq!(again.get(Path::new("/notes/b.md")).len(), 1);
        assert!(again.get(Path::new("/notes/c.md")).is_empty());
    }

    #[test]
    fn only_so_many_files_are_kept() {
        let mut memory = FoldMemory::default();
        for n in 0..FILE_LIMIT + 10 {
            memory.set(Path::new(&format!("/notes/{n}.md")), "k", true);
        }
        assert_eq!(memory.files.len(), FILE_LIMIT);
        assert!(
            !memory
                .get(Path::new(&format!("/notes/{}.md", FILE_LIMIT + 9)))
                .is_empty(),
            "the newest stays"
        );
    }
}
