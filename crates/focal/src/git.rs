//! The last committed version of a file, through the `git` command, for
//! showing what changed since the last commit.

use std::path::Path;
use std::process::Command;

const NOT_IN_GIT: &str = "This file is not in a git repository.";

/// The file's text in the last commit: empty when it is not committed yet,
/// an explanation when there is no repository or no `git`.
pub fn committed_text(path: &Path) -> Result<String, String> {
    let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else {
        return Err(NOT_IN_GIT.to_owned());
    };
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .arg("show")
        .arg(format!("HEAD:./{}", name.to_string_lossy()))
        .output()
        .map_err(|_| "Comparing with the last commit needs git, which was not found.".to_owned())?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }
    let error = String::from_utf8_lossy(&output.stderr);
    if error.contains("not a git repository") {
        Err(NOT_IN_GIT.to_owned())
    } else if error.contains("exists on disk, but not in")
        || error.contains("does not exist in")
        || error.contains("invalid object name")
        || error.contains("unknown revision")
    {
        // Not committed yet, or no commit at all: all of it is new.
        Ok(String::new())
    } else {
        Err(format!(
            "git could not show the last commit: {}",
            error.trim()
        ))
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    pub fn git(dir: &std::path::Path, args: &[&str]) {
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_AUTHOR_NAME", "Focal")
            .env("GIT_AUTHOR_EMAIL", "focal@example.com")
            .env("GIT_COMMITTER_NAME", "Focal")
            .env("GIT_COMMITTER_EMAIL", "focal@example.com")
            .output()
            .unwrap();
        assert!(status.status.success(), "{status:?}");
    }

    #[test]
    fn the_committed_text_of_a_file() {
        let dir = crate::folder::tests::temp_folder("git-committed");
        git(&dir, &["init", "-q"]);
        std::fs::write(dir.join("a b.md"), "committed\n").unwrap();
        git(&dir, &["add", "."]);
        git(&dir, &["commit", "-q", "-m", "one"]);
        std::fs::write(dir.join("a b.md"), "edited\n").unwrap();
        assert_eq!(
            committed_text(&dir.join("a b.md")).as_deref(),
            Ok("committed\n")
        );
        std::fs::write(dir.join("new.md"), "x").unwrap();
        assert_eq!(
            committed_text(&dir.join("new.md")).as_deref(),
            Ok(""),
            "not committed yet"
        );
    }

    #[test]
    fn outside_a_repository_says_so() {
        let dir = std::env::temp_dir().join(format!("focal-no-git-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.md"), "x").unwrap();
        // The temporary folder is not in a repository on macOS.
        assert_eq!(
            committed_text(&dir.join("a.md")),
            Err("This file is not in a git repository.".to_owned())
        );
    }
}
