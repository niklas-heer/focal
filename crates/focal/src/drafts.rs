//! Drafts: the text of untitled documents, kept in
//! `~/Library/Application Support/Focal/Drafts` until it is saved or
//! discarded, so logging out, a restart or a crash never loses it. The next
//! launch reopens each draft as an unsaved untitled document.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use gpui_kit::{App, Global};

#[derive(Clone, Default)]
pub struct Drafts {
    /// Where drafts are kept; `None` keeps none (tests, unless they choose).
    dir: Option<PathBuf>,
}

impl Global for Drafts {}

impl Drafts {
    pub const fn in_dir(dir: PathBuf) -> Self {
        Self { dir: Some(dir) }
    }

    /// A file for a new draft, if drafts are kept.
    pub fn new_path(&self) -> Option<PathBuf> {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let dir = self.dir.as_ref()?;
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis());
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        Some(dir.join(format!("Untitled {millis}-{n}.md")))
    }

    /// The drafts left, the oldest first.
    pub fn existing(&self) -> Vec<PathBuf> {
        let Some(dir) = &self.dir else {
            return Vec::new();
        };
        let mut drafts: Vec<PathBuf> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "md"))
            .collect();
        drafts.sort_by_cached_key(|path| std::fs::metadata(path).and_then(|m| m.modified()).ok());
        drafts
    }
}

/// Writes `text` to the draft at `path`.
pub fn write(path: &Path, text: &str) -> anyhow::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, text)?;
    Ok(())
}

/// Removes a draft that is no longer needed.
pub fn remove(path: &Path) {
    if let Err(error) = std::fs::remove_file(path)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        eprintln!("focal: removing the draft {}: {error}", path.display());
    }
}

/// Keeps drafts in Application Support.
pub fn init(cx: &mut App) {
    if let Some(home) = std::env::var_os("HOME") {
        let dir = PathBuf::from(home).join("Library/Application Support/Focal/Drafts");
        cx.set_global(Drafts::in_dir(dir));
    }
}

/// Reopens every draft left by an earlier run.
pub fn restore(cx: &mut App) {
    let drafts = cx.try_global::<Drafts>().cloned().unwrap_or_default();
    for path in drafts.existing() {
        match std::fs::read_to_string(&path) {
            Ok(text) => crate::windows::open_draft(path, text, cx),
            Err(error) => eprintln!("focal: reading the draft {}: {error}", path.display()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_drafts_get_their_own_files_and_are_listed() {
        let dir = std::env::temp_dir().join(format!("focal-drafts-unit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let drafts = Drafts::in_dir(dir.clone());
        let (a, b) = (drafts.new_path().unwrap(), drafts.new_path().unwrap());
        assert_ne!(a, b);
        write(&a, "a").unwrap();
        write(&b, "b").unwrap();
        std::fs::write(dir.join("notes.txt"), "not a draft").unwrap();
        assert_eq!(drafts.existing().len(), 2);
        remove(&a);
        remove(&a);
        assert_eq!(drafts.existing(), [b]);
        assert!(
            Drafts::default().new_path().is_none(),
            "no folder, no drafts"
        );
    }
}
