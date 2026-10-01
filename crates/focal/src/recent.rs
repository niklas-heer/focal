//! Recently opened files and folders, for File ▸ Open Recent: a JSON list
//! beside the settings, the most recent first.

use std::path::{Path, PathBuf};

use gpui_kit::{App, Global, Menu, MenuItem, actions};

/// Entries kept.
const LIMIT: usize = 10;

actions!(focal, [ClearRecent]);

/// Opens a recently opened file or folder.
#[derive(Clone, Debug, PartialEq, gpui_kit::Action)]
#[action(namespace = focal, no_json)]
pub struct OpenRecent(pub PathBuf);

#[derive(Default)]
pub struct Recent {
    paths: Vec<PathBuf>,
    /// Where the list is kept; `None` keeps it in memory only (tests).
    file: Option<PathBuf>,
}

impl Global for Recent {}

impl Recent {
    /// The list kept in `file`; empty when it is missing or unreadable.
    pub fn load_from(file: &Path) -> Self {
        let paths = std::fs::read_to_string(file)
            .ok()
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default();
        Self {
            paths,
            file: Some(file.to_owned()),
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let Some(file) = &self.file else {
            return Ok(());
        };
        if let Some(folder) = file.parent() {
            std::fs::create_dir_all(folder)?;
        }
        std::fs::write(file, serde_json::to_string_pretty(&self.paths)? + "\n")?;
        Ok(())
    }

    /// Puts `path` first.
    pub fn add(&mut self, path: PathBuf) {
        self.paths.retain(|known| *known != path);
        self.paths.insert(0, path);
        self.paths.truncate(LIMIT);
    }

    /// The entries that still exist, the most recent first.
    pub fn existing(&self) -> Vec<PathBuf> {
        self.paths.iter().filter(|p| p.exists()).cloned().collect()
    }

    pub fn clear(&mut self) {
        self.paths.clear();
    }
}

/// Menu labels: file names, with the folder added where two names are the
/// same, and folders ending in `/`.
pub fn labels(paths: &[PathBuf]) -> Vec<String> {
    let name = |path: &Path| {
        path.file_name().map_or_else(
            || path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        )
    };
    paths
        .iter()
        .map(|path| {
            let mut label = name(path);
            let shared = paths
                .iter()
                .filter(|other| other.file_name() == path.file_name())
                .count()
                > 1;
            if shared && let Some(parent) = path.parent() {
                label = format!("{label} — {}", name(parent));
            }
            if path.is_dir() && !label.ends_with('/') {
                label.push('/');
            }
            label
        })
        .collect()
}

/// `~/Library/Application Support/Focal/recent.json`.
fn file() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join("Library/Application Support/Focal/recent.json"))
}

/// Loads the list and handles its menu's actions.
pub fn init(cx: &mut App) {
    let recent = file()
        .map(|file| Recent::load_from(&file))
        .unwrap_or_default();
    cx.set_global(recent);
    cx.on_action(|action: &OpenRecent, cx| {
        // The file may have gone since the menu was made.
        if action.0.exists() {
            let request = crate::instance::Request {
                path: Some(action.0.clone()),
                ..Default::default()
            };
            crate::windows::open(request, None, cx);
        } else {
            crate::menus::set_menus(cx);
        }
    });
    cx.on_action(|_: &ClearRecent, cx| {
        let recent = cx.global_mut::<Recent>();
        recent.clear();
        if let Err(error) = recent.save() {
            eprintln!("focal: clearing recent files: {error:#}");
        }
        crate::menus::set_menus(cx);
    });
}

/// Records that `path` was opened, for the menu and the Dock.
pub fn note(path: &Path, cx: &mut App) {
    let recent = cx.default_global::<Recent>();
    recent.add(path.to_owned());
    if recent.file.is_none() {
        return;
    }
    if let Err(error) = recent.save() {
        eprintln!("focal: remembering recent files: {error:#}");
    }
    cx.add_recent_document(path);
    crate::menus::set_menus(cx);
}

/// File ▸ Open Recent.
pub fn menu(cx: &App) -> Menu {
    let paths = cx
        .try_global::<Recent>()
        .map(Recent::existing)
        .unwrap_or_default();
    let items = labels(&paths)
        .into_iter()
        .zip(paths)
        .map(|(label, path)| MenuItem::action(label, OpenRecent(path)));
    let mut menu = Menu::new("Open Recent").items(items);
    if !menu.items.is_empty() {
        menu.items.push(MenuItem::separator());
    }
    menu.items.push(MenuItem::action("Clear Menu", ClearRecent));
    menu
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("focal-recent-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn touch(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, "").unwrap();
        path
    }

    #[test]
    fn opening_again_moves_a_path_to_the_front() {
        let dir = temp("front");
        let (a, b) = (touch(&dir, "a.md"), touch(&dir, "b.md"));
        let mut recent = Recent::default();
        recent.add(a.clone());
        recent.add(b.clone());
        recent.add(a.clone());
        assert_eq!(recent.existing(), [a, b]);
    }

    #[test]
    fn only_the_latest_ten_are_kept() {
        let dir = temp("limit");
        let mut recent = Recent::default();
        for n in 0..12 {
            recent.add(touch(&dir, &format!("{n}.md")));
        }
        let existing = recent.existing();
        assert_eq!(existing.len(), LIMIT);
        assert_eq!(existing[0], dir.join("11.md"));
    }

    #[test]
    fn missing_files_are_left_out() {
        let dir = temp("missing");
        let (a, b) = (touch(&dir, "a.md"), touch(&dir, "b.md"));
        let mut recent = Recent::default();
        recent.add(a.clone());
        recent.add(b);
        std::fs::remove_file(dir.join("b.md")).unwrap();
        assert_eq!(recent.existing(), [a]);
    }

    #[test]
    fn the_list_round_trips_and_clears() {
        let dir = temp("round-trip");
        let file = dir.join("recent.json");
        let a = touch(&dir, "a.md");
        let mut recent = Recent::load_from(&file);
        recent.add(a.clone());
        recent.save().unwrap();
        assert_eq!(Recent::load_from(&file).existing(), [a]);
        recent.clear();
        recent.save().unwrap();
        assert!(Recent::load_from(&file).existing().is_empty());
    }

    #[test]
    fn same_names_are_told_apart_by_their_folder() {
        let paths = [
            PathBuf::from("/notes/work/todo.md"),
            PathBuf::from("/notes/home/todo.md"),
            PathBuf::from("/notes/ideas.md"),
            PathBuf::from("/"),
        ];
        let dir = temp("labels");
        let folder = dir.join("notes");
        std::fs::create_dir_all(&folder).unwrap();
        let mut with_folder = paths.to_vec();
        with_folder.push(folder);
        assert_eq!(
            labels(&with_folder),
            [
                "todo.md — work",
                "todo.md — home",
                "ideas.md",
                "/",
                "notes/"
            ]
        );
    }
}
