//! Reading and writing the file behind a document. The file's bytes are the
//! document: nothing is normalized on the way in or out.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Context as _, Result, bail};

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
}

impl Document {
    /// Opens `path`. A missing file opens empty and is created on first save.
    pub fn open(path: PathBuf) -> Result<(Self, String)> {
        let text = match read(&path) {
            Ok(text) => text,
            Err(error) if is_not_found(&error) => String::new(),
            Err(error) => return Err(error),
        };
        let stamp = Stamp::of(&path);
        Ok((
            Self {
                path: Some(path),
                stamp,
                saved_version: 0,
            },
            text,
        ))
    }

    pub const fn untitled() -> Self {
        Self {
            path: None,
            stamp: None,
            saved_version: 0,
        }
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
        fs::write(path, text).with_context(|| format!("writing {}", path.display()))?;
        self.stamp = Stamp::of(path);
        self.saved_version = version;
        Ok(())
    }
}

/// Reads a file as UTF-8. Other encodings are refused rather than guessed, so
/// saving can never change bytes Focal did not understand.
pub fn read(path: &Path) -> Result<String> {
    let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    match String::from_utf8(bytes) {
        Ok(text) => Ok(text),
        Err(_) => bail!("{} is not UTF-8 text", path.display()),
    }
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
    fn refuses_non_utf8() {
        let path = std::env::temp_dir().join(format!("focal-latin1-{}.md", std::process::id()));
        fs::write(&path, [0x63, 0x61, 0x66, 0xe9]).unwrap();
        assert!(Document::open(path.clone()).is_err());
        fs::remove_file(&path).unwrap();
    }
}
