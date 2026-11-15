//! Reading and writing the file behind a document. The file's bytes are the
//! document: its encoding and line endings are detected when it is read and
//! kept when it is written, so an unedited file saves back byte for byte.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Context as _, Result};
use focal_core::encoding::{self, Encoding, Format};

/// What the file looked like when Focal last read or wrote it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stamp {
    modified: SystemTime,
    len: u64,
}

impl Stamp {
    pub fn of(path: &Path) -> Option<Self> {
        let metadata = fs::metadata(path).ok()?;
        Some(Self {
            modified: metadata.modified().ok()?,
            len: metadata.len(),
        })
    }
}

#[derive(Clone, Debug)]
pub struct Document {
    /// `None` for an untitled document, such as text piped into `focal`.
    pub path: Option<PathBuf>,
    pub stamp: Option<Stamp>,
    /// The buffer version that matches the file on disk.
    pub saved_version: u64,
    /// How the file stores its text.
    pub format: Format,
    /// Something the last save had to change, to tell the user.
    pub notice: Option<String>,
}

impl Document {
    /// Opens `path`. A missing file opens empty and is created on first save.
    pub fn open(path: PathBuf) -> Result<(Self, String)> {
        let (text, format) = match read(&path) {
            Ok(read) => read,
            Err(error) if is_not_found(&error) => (String::new(), Format::default()),
            Err(error) => return Err(error),
        };
        let stamp = Stamp::of(&path);
        Ok((
            Self {
                path: Some(path),
                stamp,
                saved_version: 0,
                format,
                notice: None,
            },
            text,
        ))
    }

    pub const fn untitled() -> Self {
        Self {
            path: None,
            stamp: None,
            saved_version: 0,
            format: Format {
                encoding: Encoding::Utf8,
                bom: false,
                cr_only: false,
            },
            notice: None,
        }
    }

    /// An untitled document whose text is in no file yet, such as text
    /// piped into `focal`: unsaved from the start.
    pub const fn unsaved() -> Self {
        let mut document = Self::untitled();
        document.saved_version = u64::MAX;
        document
    }

    pub fn title(&self) -> String {
        self.path.as_deref().and_then(Path::file_name).map_or_else(
            || "Untitled".to_owned(),
            |name| name.to_string_lossy().into_owned(),
        )
    }

    pub fn save(&mut self, text: &str, version: u64) -> Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let bytes = if let Some(bytes) = encoding::encode(text, &self.format) {
            bytes
        } else {
            let name = self.format.name().unwrap_or("its encoding");
            self.notice = Some(format!(
                "Saved as UTF-8: {name} cannot hold every character of the text."
            ));
            self.format = Format {
                cr_only: self.format.cr_only,
                ..Format::default()
            };
            encoding::encode(text, &self.format).unwrap_or_else(|| text.as_bytes().to_vec())
        };
        write_replacing(path, &bytes).with_context(|| format!("writing {}", path.display()))?;
        self.stamp = Stamp::of(path);
        self.saved_version = version;
        Ok(())
    }
}

/// Writes `bytes` to `path` so that the file is never left half-written:
/// into a file beside it first, flushed to disk, which then takes the
/// original's place (keeping its permissions and metadata). A symbolic link
/// stays a link, and the file it names is replaced. Where no file can be
/// made beside it, the file is written in place.
pub fn write_replacing(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write as _;
    let is_link = fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink());
    let target = if is_link {
        fs::canonicalize(path)?
    } else {
        path.to_path_buf()
    };
    let (Some(dir), Some(name)) = (target.parent(), target.file_name()) else {
        return fs::write(&target, bytes);
    };
    let temporary = dir.join(format!(".{}.focal-save", name.to_string_lossy()));
    let written = fs::File::create(&temporary).and_then(|mut file| {
        file.write_all(bytes)?;
        file.sync_all()
    });
    if let Err(error) = written {
        let _ = fs::remove_file(&temporary);
        // A folder Focal may not create files in, but a file it may write.
        return if error.kind() == ErrorKind::PermissionDenied {
            fs::write(&target, bytes)
        } else {
            Err(error)
        };
    }
    let replaced = if target.exists() {
        crate::mac::replace_file(&target, &temporary).map_err(std::io::Error::other)
    } else {
        fs::rename(&temporary, &target)
    };
    if replaced.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    replaced
}

/// Whether windows watch their files and folders. UI tests do not: FSEvents
/// reports on its own thread, which GPUI's deterministic test scheduler
/// rejects, so they call `check_disk` and rescan directly instead. The
/// watchers themselves have their own tests.
pub const WATCH_FILES: bool = !cfg!(test);

/// Watches `path` for changes, including being replaced by a rename (editors
/// and agents often write a temporary file and rename it over the original).
/// The file's folder is watched, because a rename replaces the file itself;
/// events are matched by file name, since FSEvents may report the folder
/// through a different path (such as `/private/var` for `/var`).
pub fn watch(path: &Path) -> Result<(notify::RecommendedWatcher, async_channel::Receiver<()>)> {
    use notify::{RecursiveMode, Watcher as _};
    let name = path
        .file_name()
        .context("the path has no file name")?
        .to_owned();
    let folder = path.parent().context("the file has no folder")?;
    let (tx, rx) = async_channel::unbounded();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        if let Ok(event) = event
            && event
                .paths
                .iter()
                .any(|p| p.file_name() == Some(name.as_os_str()))
        {
            let _ = tx.try_send(());
        }
    })?;
    watcher.watch(folder, RecursiveMode::NonRecursive)?;
    Ok((watcher, rx))
}

/// Reads a file in whatever encoding it uses (see
/// [`focal_core::encoding`]), and how it stores its text.
pub fn read(path: &Path) -> Result<(String, Format)> {
    let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(encoding::decode(&bytes))
}

fn is_not_found(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<std::io::Error>()
        .is_some_and(|error| error.kind() == ErrorKind::NotFound)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saving_replaces_the_file_and_keeps_its_metadata() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = crate::folder::tests::temp_folder("atomic-save");
        let path = dir.join("note.md");
        fs::write(&path, "old").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let tagged = std::process::Command::new("xattr")
            .args(["-w", "com.focal.test", "kept"])
            .arg(&path)
            .status()
            .is_ok_and(|s| s.success());
        write_replacing(&path, b"new").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        if tagged {
            let out = std::process::Command::new("xattr")
                .args(["-p", "com.focal.test"])
                .arg(&path)
                .output()
                .unwrap();
            assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "kept");
        }
        let names: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, ["note.md"], "no temporary file is left");
    }

    #[test]
    fn saving_through_a_link_keeps_the_link() {
        let dir = crate::folder::tests::temp_folder("atomic-link");
        let real = dir.join("real.md");
        let link = dir.join("link.md");
        fs::write(&real, "old").unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();
        write_replacing(&link, b"new").unwrap();
        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read_to_string(&real).unwrap(), "new");
    }

    #[test]
    fn saving_creates_a_missing_file() {
        let dir = crate::folder::tests::temp_folder("atomic-new");
        let path = dir.join("new.md");
        write_replacing(&path, b"hello").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "hello");
    }

    fn next_event(rx: &async_channel::Receiver<()>) -> bool {
        let rx = rx.clone();
        let (tx, done) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(rx.recv_blocking().is_ok());
        });
        done.recv_timeout(std::time::Duration::from_secs(5))
            .unwrap_or(false)
    }

    #[test]
    fn watching_reports_writes_and_atomic_renames() {
        let dir = std::env::temp_dir().join(format!("focal-watch-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("watched.md");
        fs::write(&path, "one").unwrap();
        let (_watcher, rx) = watch(&path).unwrap();
        fs::write(&path, "two").unwrap();
        assert!(next_event(&rx), "a write is reported");
        while rx.try_recv().is_ok() {}
        let temp = dir.join(".watched.md.tmp");
        fs::write(&temp, "three").unwrap();
        fs::rename(&temp, &path).unwrap();
        assert!(next_event(&rx), "an atomic rename is reported");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn round_trips_bytes_exactly() {
        let dir = std::env::temp_dir().join(format!("focal-doc-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("round-trip.md");
        let original = "# Title\r\n\r\n\ttabs  and trailing spaces  \r\nno final newline — ünïcödé";
        fs::write(&path, original).unwrap();

        let (mut document, text) = Document::open(path.clone()).unwrap();
        document.save(&text, 0).unwrap();
        assert_eq!(fs::read(&path).unwrap(), original.as_bytes());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn missing_file_opens_empty() {
        let path = std::env::temp_dir().join("focal-definitely-missing.md");
        let (document, text) = Document::open(path).unwrap();
        assert!(text.is_empty());
        assert!(document.stamp.is_none());
    }

    #[test]
    fn other_encodings_open_and_save_back_as_they_were() {
        let path = std::env::temp_dir().join(format!("focal-latin1-{}.md", std::process::id()));
        fs::write(&path, b"caf\xe9\n").unwrap();
        let (mut document, text) = Document::open(path.clone()).unwrap();
        assert_eq!(text, "café\n");
        assert_eq!(document.format.name(), Some("Windows-1252"));
        document.save("café!\n", 1).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"caf\xe9!\n");
        assert_eq!(document.notice, None);
        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn a_character_the_encoding_cannot_hold_saves_as_utf8_and_says_so() {
        let path = std::env::temp_dir().join(format!("focal-latin1-up-{}.md", std::process::id()));
        fs::write(&path, b"caf\xe9\n").unwrap();
        let (mut document, _) = Document::open(path.clone()).unwrap();
        document.save("café ✓\n", 1).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "café ✓\n");
        assert_eq!(document.format.name(), None, "now UTF-8");
        assert!(
            document
                .notice
                .as_deref()
                .is_some_and(|n| n.contains("UTF-8"))
        );
        fs::remove_file(&path).unwrap();
    }
}
