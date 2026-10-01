//! Command-line tools Focal draws with when they are installed (`dot`,
//! `d2`, `plantuml`): found on `PATH` and in Homebrew's folders, which apps
//! started from Finder do not see, and run with a time limit.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

/// How long a tool may take before Focal gives up on it.
pub const TIMEOUT: Duration = Duration::from_secs(10);

/// Where `name` is installed, if anywhere; looked up once.
pub fn find(name: &str) -> Option<PathBuf> {
    static FOUND: OnceLock<Mutex<HashMap<String, Option<PathBuf>>>> = OnceLock::new();
    let found = FOUND.get_or_init(Mutex::default);
    let mut found = found
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    found
        .entry(name.to_owned())
        .or_insert_with(|| {
            let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
                .map(|path| std::env::split_paths(&path).collect())
                .unwrap_or_default();
            dirs.extend(["/opt/homebrew/bin", "/usr/local/bin"].map(PathBuf::from));
            find_in(name, &dirs)
        })
        .clone()
}

/// The first executable `name` in `dirs`.
pub fn find_in(name: &str, dirs: &[PathBuf]) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt as _;
    dirs.iter().map(|dir| dir.join(name)).find(|path| {
        std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    })
}

/// Runs `tool` with `args`, `input` on its standard input, and returns its
/// standard output, or its error output (or a timeout) as the message.
pub fn run(tool: &Path, args: &[&str], input: &str, timeout: Duration) -> Result<String, String> {
    use std::io::{Read as _, Write as _};
    use std::process::{Command, Stdio};
    let name = tool
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let mut child = Command::new(tool)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("could not start {name}: {error}"))?;
    let mut stdin = child.stdin.take();
    let input = input.to_owned();
    let writer = std::thread::spawn(move || {
        if let Some(stdin) = &mut stdin {
            let _ = stdin.write_all(input.as_bytes());
        }
    });
    let reader = |pipe: Option<Box<dyn std::io::Read + Send>>| {
        std::thread::spawn(move || {
            let mut out = Vec::new();
            if let Some(mut pipe) = pipe {
                let _ = pipe.read_to_end(&mut out);
            }
            out
        })
    };
    let stdout = reader(
        child
            .stdout
            .take()
            .map(|p| Box::new(p) as Box<dyn std::io::Read + Send>),
    );
    let stderr = reader(
        child
            .stderr
            .take()
            .map(|p| Box::new(p) as Box<dyn std::io::Read + Send>),
    );
    let deadline = std::time::Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("{name} took too long"));
            }
            Err(error) => return Err(format!("{name}: {error}")),
        }
    };
    let _ = writer.join();
    let out = stdout.join().unwrap_or_default();
    let err = stderr.join().unwrap_or_default();
    if status.success() {
        Ok(String::from_utf8_lossy(&out).into_owned())
    } else {
        let message = String::from_utf8_lossy(&err).trim().to_owned();
        Err(if message.is_empty() {
            format!("{name} failed ({status})")
        } else {
            message
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt as _;

    fn fake_tool(name: &str, script: &str) -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!("focal-tools-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let tool = dir.join(name);
        std::fs::write(&tool, format!("#!/bin/sh\n{script}\n")).unwrap();
        std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();
        (dir, tool)
    }

    #[test]
    fn tools_are_found_in_the_folders_given() {
        let (dir, tool) = fake_tool("focaltool", "cat");
        assert_eq!(
            find_in("focaltool", &[PathBuf::from("/nonexistent"), dir]),
            Some(tool)
        );
        assert_eq!(find_in("focal-no-such-tool", &[]), None);
    }

    #[test]
    fn a_tool_gets_the_input_and_gives_its_output() {
        let (_, tool) = fake_tool("echoing", "tr a-z A-Z");
        assert_eq!(run(&tool, &[], "shout", TIMEOUT).as_deref(), Ok("SHOUT"));
    }

    #[test]
    fn a_failing_tool_says_why() {
        let (_, tool) = fake_tool("failing", "echo 'syntax error on line 2' >&2; exit 1");
        assert_eq!(
            run(&tool, &[], "", TIMEOUT),
            Err("syntax error on line 2".into())
        );
    }

    #[test]
    fn a_slow_tool_times_out() {
        let (_, tool) = fake_tool("sleepy", "sleep 5");
        let error = run(&tool, &[], "", Duration::from_millis(200)).unwrap_err();
        assert!(error.contains("took too long"), "{error}");
    }
}
