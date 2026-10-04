//! Word-count goals, one per file, kept in
//! `~/Library/Application Support/Focal/goals.json` rather than in the file,
//! which Focal never rewrites.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use gpui_kit::{App, Global};

/// Files whose goals are kept; the oldest give way beyond this.
const FILE_LIMIT: usize = 500;

/// Sets the open document's word goal; `None` removes it.
#[derive(Clone, Debug, PartialEq, gpui_kit::Action)]
#[action(namespace = focal, no_json)]
pub struct SetGoal(pub Option<usize>);

#[derive(Default)]
pub struct Goals {
    /// By file path: the goal, and when it was last set.
    goals: BTreeMap<String, (u64, usize)>,
    /// Where the goals are kept; `None` keeps them in memory only (tests).
    file: Option<PathBuf>,
}

impl Global for Goals {}

impl Goals {
    /// The goals kept in `file`; none when it is missing or unreadable.
    pub fn load_from(file: &Path) -> Self {
        let goals = std::fs::read_to_string(file)
            .ok()
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default();
        Self {
            goals,
            file: Some(file.to_owned()),
        }
    }

    pub fn get(&self, path: &Path) -> Option<usize> {
        self.goals
            .get(&path.to_string_lossy().into_owned())
            .map(|(_, goal)| *goal)
    }

    /// Sets or removes the goal for `path`, and saves.
    pub fn set(&mut self, path: &Path, goal: Option<usize>) {
        let key = path.to_string_lossy().into_owned();
        match goal.filter(|goal| *goal > 0) {
            Some(goal) => {
                let stamp = self
                    .goals
                    .values()
                    .map(|(stamp, _)| *stamp)
                    .max()
                    .unwrap_or(0)
                    + 1;
                self.goals.insert(key, (stamp, goal));
            }
            None => {
                self.goals.remove(&key);
            }
        }
        while self.goals.len() > FILE_LIMIT {
            let oldest = self
                .goals
                .iter()
                .min_by_key(|(_, (stamp, _))| *stamp)
                .map(|(path, _)| path.clone());
            if let Some(oldest) = oldest {
                self.goals.remove(&oldest);
            }
        }
        if let Some(file) = &self.file
            && let Err(error) = save(file, &self.goals)
        {
            eprintln!("focal: saving word goals: {error:#}");
        }
    }
}

fn save(file: &Path, goals: &BTreeMap<String, (u64, usize)>) -> anyhow::Result<()> {
    if let Some(folder) = file.parent() {
        std::fs::create_dir_all(folder)?;
    }
    std::fs::write(file, serde_json::to_string(goals)?)?;
    Ok(())
}

/// The goal for `path`, if it has one.
pub fn goal(path: Option<&Path>, cx: &App) -> Option<usize> {
    cx.try_global::<Goals>()?.get(path?)
}

/// Sets or removes the goal for `path`.
pub fn set(path: &Path, goal: Option<usize>, cx: &mut App) {
    if !cx.has_global::<Goals>() {
        cx.set_global(Goals::default());
    }
    cx.global_mut::<Goals>().set(path, goal);
}

/// How far `words` are towards `goal`, from 0 to 1.
pub fn progress(words: usize, goal: usize) -> f32 {
    if goal == 0 {
        return 1.;
    }
    #[allow(clippy::cast_precision_loss)]
    let progress = words as f32 / goal as f32;
    progress.min(1.)
}

/// Loads the goals from Application Support.
pub fn init(cx: &mut App) {
    if let Some(home) = std::env::var_os("HOME") {
        let file = PathBuf::from(home).join("Library/Application Support/Focal/goals.json");
        cx.set_global(Goals::load_from(&file));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn goals_are_kept_per_file_across_launches() {
        let dir = std::env::temp_dir().join(format!("focal-goals-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let file = dir.join("goals.json");
        let mut goals = Goals::load_from(&file);
        goals.set(Path::new("/notes/a.md"), Some(1000));
        goals.set(Path::new("/notes/b.md"), Some(500));
        goals.set(Path::new("/notes/b.md"), None);
        let again = Goals::load_from(&file);
        assert_eq!(again.get(Path::new("/notes/a.md")), Some(1000));
        assert_eq!(again.get(Path::new("/notes/b.md")), None, "removed");
    }

    #[test]
    fn progress_stops_at_the_goal() {
        assert!((progress(250, 1000) - 0.25).abs() < f32::EPSILON);
        assert!((progress(2000, 1000) - 1.).abs() < f32::EPSILON);
    }
}
