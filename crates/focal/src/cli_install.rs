//! "Install Command Line Tool…": puts `focal` on `PATH` as a link to the
//! binary inside `Focal.app`.

use std::io::ErrorKind;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context as _, Result, bail};
use gpui_kit::{App, actions};

/// Where the command goes: on `PATH` in every shell, and where VS Code and
/// Zed put theirs. `FOCAL_LINK_DIR` overrides it, to try the menu items
/// without touching `/usr/local/bin`.
fn link_dir() -> PathBuf {
    std::env::var_os("FOCAL_LINK_DIR")
        .map_or_else(|| PathBuf::from("/usr/local/bin"), PathBuf::from)
}

#[derive(Debug, PartialEq, Eq)]
pub enum Installed {
    Linked,
    AlreadyLinked,
}

/// A path to the `focal` binary of some `Focal.app`.
fn is_focal(path: &Path) -> bool {
    path.ends_with("Focal.app/Contents/MacOS/focal")
}

/// The `focal` inside the running `Focal.app`, if Focal runs from a bundle.
pub fn bundled_binary() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?.canonicalize().ok()?;
    is_focal(&exe).then_some(exe)
}

/// Where the `focal` command runs this Focal: on `PATH`, in Homebrew's
/// folders or where "Install Command Line Tool…" puts it.
pub fn command_path() -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect())
        .unwrap_or_default();
    dirs.extend([PathBuf::from("/opt/homebrew/bin"), link_dir()]);
    dirs.into_iter()
        .map(|dir| dir.join("focal"))
        .find(|path| path.canonicalize().is_ok_and(|target| is_focal(&target)))
}

/// Whether `path` is the link "Install Command Line Tool…" makes, which
/// "Uninstall" may remove.
pub fn is_own_link(path: &Path) -> bool {
    path == link_dir().join("focal")
}

/// Links `link_dir/focal` to `target`. A link to another Focal is replaced;
/// any other file is left alone and reported. When the folder is missing or
/// not writable, `escalate` runs one shell script as an administrator.
pub fn install_with(
    link_dir: &Path,
    target: &Path,
    escalate: impl FnOnce(&str) -> Result<()>,
) -> Result<Installed> {
    let link = link_dir.join("focal");
    match std::fs::symlink_metadata(&link) {
        Ok(meta) if meta.file_type().is_symlink() => {
            let current = std::fs::read_link(&link)?;
            if current == target {
                return Ok(Installed::AlreadyLinked);
            }
            if !is_focal(&current) {
                bail!(
                    "{} links to {}, which is not Focal's; remove it first",
                    link.display(),
                    current.display()
                );
            }
        }
        Ok(_) => bail!("{} is not Focal's; remove it first", link.display()),
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let linked = std::fs::remove_file(&link)
        .or_else(|error| {
            if error.kind() == ErrorKind::NotFound {
                Ok(())
            } else {
                Err(error)
            }
        })
        .and_then(|()| symlink(target, &link));
    match linked {
        Ok(()) => {}
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::PermissionDenied | ErrorKind::NotFound
            ) =>
        {
            escalate(&format!(
                "mkdir -p {dir} && ln -sf {target} {link}",
                dir = quote(link_dir),
                target = quote(target),
                link = quote(&link),
            ))?;
        }
        Err(error) => return Err(error).with_context(|| format!("linking {}", link.display())),
    }
    Ok(Installed::Linked)
}

/// Removes `link_dir/focal` if it links to a Focal. Returns whether there
/// was one.
pub fn uninstall_with(link_dir: &Path, escalate: impl FnOnce(&str) -> Result<()>) -> Result<bool> {
    let link = link_dir.join("focal");
    match std::fs::read_link(&link) {
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(false),
        Ok(current) if is_focal(&current) => {}
        // Another link, or a file that is no link at all.
        _ => bail!("{} is not Focal's; leaving it", link.display()),
    }
    match std::fs::remove_file(&link) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::PermissionDenied => {
            escalate(&format!("rm -f {}", quote(&link)))?;
        }
        Err(error) => return Err(error.into()),
    }
    Ok(true)
}

actions!(focal, [InstallCommand, UninstallCommand]);

/// Registers the menu actions. They do anything only in a bundled Focal.
pub fn init(cx: &mut App) {
    cx.on_action(|_: &InstallCommand, cx| {
        let result = match bundled_binary() {
            Some(target) => install_with(&link_dir(), &target, as_administrator).map(|_| {
                "Open a new terminal and type focal notes.md, or focal . for a folder.".to_owned()
            }),
            None => Err(anyhow::anyhow!("Only Focal.app can install the command.")),
        };
        report(cx, "The focal command is installed.", result);
    });
    cx.on_action(|_: &UninstallCommand, cx| {
        let result = uninstall_with(&link_dir(), as_administrator).map(|removed| {
            if removed {
                format!("It was removed from {}.", link_dir().display())
            } else {
                "It was not installed.".to_owned()
            }
        });
        report(cx, "The focal command is uninstalled.", result);
    });
}

/// Shows the outcome in an alert.
fn report(cx: &mut App, success: &str, result: Result<String>) {
    let (warning, message, detail) = match result {
        Ok(detail) => (false, success.to_owned(), detail),
        Err(error) if error.to_string() == "cancelled" => return,
        Err(error) => (
            true,
            "The focal command could not be changed.".to_owned(),
            format!("{error:#}"),
        ),
    };
    cx.spawn(async move |_| crate::mac::alert(&message, &detail, warning))
        .detach();
}

/// Runs a shell script as an administrator, through macOS's password prompt.
pub fn as_administrator(script: &str) -> Result<()> {
    let applescript = format!(
        "do shell script \"{}\" with administrator privileges",
        script.replace('\\', "\\\\").replace('"', "\\\"")
    );
    let output = Command::new("osascript")
        .arg("-e")
        .arg(applescript)
        .output()
        .context("running osascript")?;
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr);
        if message.contains("-128") {
            bail!("cancelled");
        }
        bail!("{}", message.trim());
    }
    Ok(())
}

/// Quotes a path for `sh`.
fn quote(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::os::unix::fs::{PermissionsExt as _, symlink};
    use std::path::PathBuf;

    use super::*;

    const APP: &str = "/Applications/Focal.app/Contents/MacOS/focal";

    fn never(_: &str) -> Result<()> {
        panic!("no administrator prompt expected")
    }

    fn dir(name: &str) -> PathBuf {
        crate::folder::tests::temp_folder(&format!("cli-{name}"))
    }

    #[test]
    fn links_focal_and_notices_an_existing_link() {
        let bin = dir("fresh");
        assert_eq!(
            install_with(&bin, Path::new(APP), never).unwrap(),
            Installed::Linked
        );
        assert_eq!(
            std::fs::read_link(bin.join("focal")).unwrap(),
            PathBuf::from(APP)
        );
        assert_eq!(
            install_with(&bin, Path::new(APP), never).unwrap(),
            Installed::AlreadyLinked
        );
    }

    #[test]
    fn replaces_a_link_to_another_focal_but_not_a_foreign_file() {
        let bin = dir("replace");
        symlink(
            "/Users/me/Downloads/Focal.app/Contents/MacOS/focal",
            bin.join("focal"),
        )
        .unwrap();
        assert_eq!(
            install_with(&bin, Path::new(APP), never).unwrap(),
            Installed::Linked
        );
        assert_eq!(
            std::fs::read_link(bin.join("focal")).unwrap(),
            PathBuf::from(APP)
        );

        let bin = dir("foreign");
        std::fs::write(bin.join("focal"), "#!/bin/sh\n").unwrap();
        let error = install_with(&bin, Path::new(APP), never).unwrap_err();
        assert!(error.to_string().contains("not Focal's"), "{error}");
        assert_eq!(
            std::fs::read_to_string(bin.join("focal")).unwrap(),
            "#!/bin/sh\n"
        );
    }

    #[test]
    fn an_unwritable_folder_asks_for_an_administrator_once() {
        let bin = dir("locked");
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o555)).unwrap();
        let script = RefCell::new(String::new());
        let installed = install_with(&bin, Path::new(APP), |s| {
            script.replace(s.to_owned());
            Ok(())
        });
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(installed.unwrap(), Installed::Linked);
        let script = script.into_inner();
        assert!(
            script.contains("mkdir -p") && script.contains("ln -sf"),
            "{script}"
        );
        assert!(script.contains(APP));
    }

    #[test]
    fn uninstall_removes_only_focals_link() {
        let bin = dir("uninstall");
        symlink(APP, bin.join("focal")).unwrap();
        assert!(uninstall_with(&bin, never).unwrap());
        assert!(!bin.join("focal").exists());
        std::fs::write(bin.join("focal"), "mine").unwrap();
        assert!(uninstall_with(&bin, never).is_err());
        assert!(bin.join("focal").exists());
        std::fs::remove_file(bin.join("focal")).unwrap();
        assert!(!uninstall_with(&bin, never).unwrap(), "nothing to remove");
    }
}
