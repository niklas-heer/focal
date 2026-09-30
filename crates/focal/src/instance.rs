//! One running Focal: `focal` hands each request to the running instance over
//! a Unix socket and, with `--wait`, blocks until that window closes.

use std::hash::{DefaultHasher, Hash as _, Hasher as _};
use std::io::{self, BufRead as _, BufReader, Write as _};
use std::os::unix::ffi::OsStringExt as _;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// What a `focal` call asks the running instance to open.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Request {
    /// A file or folder, absolute.
    pub path: Option<PathBuf>,
    /// Text for a new untitled document, such as piped standard input.
    pub untitled: Option<String>,
    /// Keep the connection until the window closes.
    pub wait: bool,
}

/// The instance's socket. Each Focal binary gets its own, so a development
/// build never forwards to the installed app.
pub fn socket_path() -> PathBuf {
    let exe = std::env::current_exe()
        .and_then(std::fs::canonicalize)
        .unwrap_or_default();
    let mut hasher = DefaultHasher::new();
    exe.hash(&mut hasher);
    std::env::temp_dir().join(format!("focal-{:08x}.sock", hasher.finish() & 0xffff_ffff))
}

fn encode(request: &Request) -> String {
    let mut line = serde_json::to_string(request).unwrap_or_default();
    line.push('\n');
    line
}

fn decode(line: &str) -> io::Result<Request> {
    serde_json::from_str(line.trim_end()).map_err(io::Error::other)
}

/// Sends `request` to the running instance. With `wait`, returns when the
/// window closes or the instance goes away. Fails when no instance listens,
/// or it did not confirm (it may be quitting).
pub fn forward(socket: &Path, request: &Request) -> io::Result<()> {
    let mut stream = UnixStream::connect(socket)?;
    stream.write_all(encode(request).as_bytes())?;
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let answer = line.trim_end();
    if let Some(reason) = answer.strip_prefix(FAILED) {
        return Err(io::Error::other(reason.trim().to_owned()));
    }
    if answer != OPENED {
        return Err(io::ErrorKind::UnexpectedEof.into());
    }
    if request.wait {
        line.clear();
        // "closed", or the end of the stream when Focal quits.
        reader.read_line(&mut line)?;
    }
    Ok(())
}

const OPENED: &str = "opened";
const CLOSED: &str = "closed";
const FAILED: &str = "failed";

/// Listens at `socket`, taking over a stale socket file left by an instance
/// that is gone. Fails with `AddrInUse` while another instance answers there.
pub fn listen(socket: &Path) -> io::Result<UnixListener> {
    match UnixListener::bind(socket) {
        Err(error) if error.kind() == io::ErrorKind::AddrInUse => {
            if UnixStream::connect(socket).is_ok() {
                return Err(error);
            }
            std::fs::remove_file(socket)?;
            UnixListener::bind(socket)
        }
        result => result,
    }
}

/// Waits for the next request.
pub fn accept(listener: &UnixListener) -> io::Result<(Request, Responder)> {
    let (stream, _) = listener.accept()?;
    let mut line = String::new();
    BufReader::new(&stream).read_line(&mut line)?;
    Ok((decode(&line)?, Responder { stream }))
}

/// Answers one request: once when its window is open, once when it closes.
/// Dropping it ends the connection, which also ends a wait.
pub struct Responder {
    stream: UnixStream,
}

impl Responder {
    pub fn opened(&mut self) {
        let _ = writeln!(self.stream, "{OPENED}");
    }

    pub fn closed(mut self) {
        let _ = writeln!(self.stream, "{CLOSED}");
    }

    /// Nothing opened; `focal` prints the reason.
    pub fn failed(mut self, reason: &str) {
        let reason = reason.replace('\n', " ");
        let _ = writeln!(self.stream, "{FAILED} {reason}");
    }
}

/// The path of a `file://` URL, as Finder sends to "Open With".
pub fn file_url_path(url: &str) -> Option<PathBuf> {
    let encoded = url.strip_prefix("file://")?;
    let encoded = encoded.strip_prefix("localhost").unwrap_or(encoded);
    let bytes = encoded.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(byte) = encoded
                .get(i + 1..i + 3)
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
        {
            decoded.push(byte);
            i += 3;
        } else {
            decoded.push(bytes[i]);
            i += 1;
        }
    }
    let path = PathBuf::from(std::ffi::OsString::from_vec(decoded));
    let path = path.components().collect::<PathBuf>();
    path.is_absolute().then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A socket path in the temporary folder, removed when dropped.
    struct Socket(PathBuf);

    impl std::ops::Deref for Socket {
        type Target = Path;
        fn deref(&self) -> &Path {
            &self.0
        }
    }

    impl AsRef<Path> for Socket {
        fn as_ref(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Socket {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    fn socket(name: &str) -> Socket {
        let path =
            std::env::temp_dir().join(format!("focal-test-{}-{name}.sock", std::process::id()));
        let _ = std::fs::remove_file(&path);
        Socket(path)
    }
    #[test]
    fn a_request_travels_as_one_json_line() {
        let request = Request {
            path: Some("/tmp/a.md".into()),
            untitled: None,
            wait: true,
        };
        let line = encode(&request);
        assert!(line.ends_with('\n') && !line.trim_end().contains('\n'));
        assert_eq!(decode(&line).unwrap(), request);
    }

    #[test]
    fn forwarding_reaches_the_server_and_waits_for_the_window() {
        let path = socket("wait");
        let listener = listen(&path).unwrap();
        let server = std::thread::spawn(move || {
            let (request, mut responder) = accept(&listener).unwrap();
            responder.opened();
            std::thread::sleep(std::time::Duration::from_millis(50));
            responder.closed();
            request
        });
        let request = Request {
            path: Some("/tmp/a.md".into()),
            untitled: None,
            wait: true,
        };
        forward(&path, &request).unwrap();
        assert_eq!(server.join().unwrap(), request);
    }

    #[test]
    fn a_waiting_call_returns_when_the_instance_goes_away() {
        let path = socket("gone");
        let listener = listen(&path).unwrap();
        let server = std::thread::spawn(move || {
            let (_, mut responder) = accept(&listener).unwrap();
            responder.opened();
            // Dropped without `closed`, as when Focal quits.
        });
        let request = Request {
            wait: true,
            ..Request::default()
        };
        forward(&path, &request).unwrap();
        server.join().unwrap();
    }

    #[test]
    fn a_stale_socket_is_refused_then_replaced() {
        let path = socket("stale");
        drop(std::os::unix::net::UnixListener::bind(&path).unwrap());
        assert!(path.exists(), "the socket file outlives its listener");
        assert!(forward(&path, &Request::default()).is_err());
        assert!(listen(&path).is_ok(), "a new instance takes the path over");
    }

    #[test]
    fn file_urls_become_paths() {
        assert_eq!(
            file_url_path("file:///Users/n/My%20Notes/caf%C3%A9.md"),
            Some(PathBuf::from("/Users/n/My Notes/café.md"))
        );
        assert_eq!(
            file_url_path("file:///Users/n/notes/"),
            Some(PathBuf::from("/Users/n/notes"))
        );
        assert_eq!(file_url_path("https://example.com/a.md"), None);
    }

    #[test]
    fn a_failure_reaches_the_caller() {
        let path = socket("failed");
        let listener = listen(&path).unwrap();
        let server = std::thread::spawn(move || {
            let (_, responder) = accept(&listener).unwrap();
            responder.failed("x.md is not UTF-8 text");
        });
        let error = forward(&path, &Request::default()).unwrap_err();
        assert_eq!(error.to_string(), "x.md is not UTF-8 text");
        server.join().unwrap();
    }

    #[test]
    fn a_live_instance_keeps_its_socket() {
        let path = socket("live");
        let _first = listen(&path).unwrap();
        assert_eq!(listen(&path).unwrap_err().kind(), io::ErrorKind::AddrInUse);
    }
}
