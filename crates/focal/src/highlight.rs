//! Syntax highlighting for fenced code blocks, through GPUI Kit's tree-sitter
//! highlighter (the grammars are Cargo features of `gpui-kit`).

use std::ops::Range;

use gpui_kit::HighlightStyle;
use gpui_kit::component::Rope;
use gpui_kit::component::highlighter::{HighlightTheme, SyntaxHighlighter};

/// The grammar for a code fence's info string.
pub fn language_name(fence: &str) -> Option<&'static str> {
    let name = fence
        .split([',', ' ', '{'])
        .next()?
        .trim()
        .to_ascii_lowercase();
    Some(match name.as_str() {
        "rust" | "rs" => "rust",
        "python" | "py" => "python",
        "javascript" | "js" | "mjs" | "cjs" | "jsx" => "javascript",
        "typescript" | "ts" => "typescript",
        "tsx" => "tsx",
        "go" | "golang" => "go",
        "bash" | "sh" | "shell" | "zsh" | "console" => "bash",
        "c" | "h" => "c",
        "cpp" | "c++" | "cc" | "hpp" => "cpp",
        "css" => "css",
        "diff" | "patch" => "diff",
        "html" | "xml" | "svg" => "html",
        "java" => "java",
        "kotlin" | "kt" => "kotlin",
        "lua" => "lua",
        "make" | "makefile" => "make",
        "php" => "php",
        "ruby" | "rb" => "ruby",
        "sql" => "sql",
        "swift" => "swift",
        "toml" => "toml",
        "yaml" | "yml" => "yaml",
        "zig" => "zig",
        _ => return None,
    })
}

/// Highlights a code block as a whole (so multi-line strings and comments are
/// right) and returns each line's styles as byte ranges within that line.
pub fn highlight_block(
    language: &str,
    lines: &[&str],
    dark: bool,
) -> Vec<Vec<(Range<usize>, HighlightStyle)>> {
    let code = lines.join("\n");
    let mut highlighter = SyntaxHighlighter::new(language);
    highlighter.update(None, &Rope::from_str(&code), None);
    let theme = if dark {
        HighlightTheme::default_dark()
    } else {
        HighlightTheme::default_light()
    };
    let styles = highlighter.styles(&(0..code.len()), &*theme);
    let mut starts = Vec::with_capacity(lines.len());
    let mut offset = 0;
    for line in lines {
        starts.push(offset);
        offset += line.len() + 1;
    }
    let mut per_line = vec![Vec::new(); lines.len()];
    for (range, style) in styles {
        for (index, start) in starts.iter().enumerate() {
            let end = start + lines[index].len();
            let clipped = range.start.max(*start)..range.end.min(end);
            if clipped.start < clipped.end {
                per_line[index].push((clipped.start - start..clipped.end - start, style));
            }
        }
    }
    per_line
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_common_fence_names() {
        assert_eq!(language_name("rs"), Some("rust"));
        assert_eq!(language_name("Python"), Some("python"));
        assert_eq!(language_name("sh"), Some("bash"));
        assert_eq!(language_name("yml"), Some("yaml"));
        assert_eq!(language_name("rust,ignore"), Some("rust"));
        assert_eq!(language_name("klingon"), None);
    }

    #[test]
    fn highlights_keywords_per_line() {
        let styles = highlight_block("rust", &["fn main() {", "    let x = 1;", "}"], true);
        assert_eq!(styles.len(), 3);
        assert!(
            styles[0].iter().any(|(range, _)| *range == (0..2)),
            "`fn` is styled: {:?}",
            styles[0]
        );
        assert!(
            styles[1].iter().any(|(range, _)| *range == (4..7)),
            "`let` is styled: {:?}",
            styles[1]
        );
    }
}
