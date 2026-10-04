//! The tags a document carries, for the sidebar: `#name` and `#name/sub` in
//! its text (Bear, Obsidian), outside code, and the `tags:` of its front
//! matter.

use std::collections::BTreeSet;

/// The document's tags, without `#`, in order and each once.
pub fn document_tags(text: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut lines = text.lines().peekable();
    // Front matter: `tags: a, b`, `tags: [a, b]`, or `- a` lines after it.
    if lines.peek().is_some_and(|line| line.trim_end() == "---") {
        lines.next();
        let mut in_tags = false;
        for line in lines.by_ref() {
            let trimmed = line.trim();
            if trimmed == "---" || trimmed == "..." {
                break;
            }
            if let Some(value) = line
                .strip_prefix("tags:")
                .or_else(|| line.strip_prefix("tag:"))
            {
                in_tags = true;
                let value = value.trim().trim_start_matches('[').trim_end_matches(']');
                found.extend(split_tags(value));
            } else if in_tags && let Some(item) = trimmed.strip_prefix("- ") {
                found.extend(split_tags(item));
            } else if !line.starts_with(' ') {
                in_tags = false;
            }
        }
    }
    let mut fence: Option<&str> = None;
    for line in lines {
        let trimmed = line.trim_start();
        if let Some(open) = fence {
            if trimmed.starts_with(open) {
                fence = None;
            }
            continue;
        }
        if trimmed.starts_with("```") {
            fence = Some("```");
            continue;
        }
        if trimmed.starts_with("~~~") {
            fence = Some("~~~");
            continue;
        }
        let line = without_inline_code(line);
        for range in crate::analysis::tags(&line) {
            let name = line[range.start + 1..range.end].trim_end_matches('/');
            if !name.is_empty() {
                found.insert(name.to_owned());
            }
        }
    }
    found
}

/// Tags written as a list: `a, b` or `"a" 'b'`, with or without `#`.
fn split_tags(value: &str) -> impl Iterator<Item = String> + '_ {
    value
        .split([',', ' '])
        .map(|tag| tag.trim().trim_matches(['"', '\'']).trim_start_matches('#'))
        .filter(|tag| !tag.is_empty())
        .map(str::to_owned)
}

/// The line with inline code spans blanked out, so `#x` in code is no tag.
fn without_inline_code(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut in_code = false;
    for c in line.chars() {
        if c == '`' {
            in_code = !in_code;
            out.push(' ');
        } else if in_code {
            out.push(' ');
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(text: &str) -> Vec<String> {
        document_tags(text).into_iter().collect()
    }

    #[test]
    fn tags_in_text_but_not_in_code_headings_or_anchors() {
        let text = "# Title\n\nAn #idea and #project/focal.\n\n`#not` here, url#anchor, #123\n\n```\n#nope\n```\n";
        assert_eq!(tags(text), ["idea", "project/focal"]);
    }

    #[test]
    fn tags_in_front_matter() {
        assert_eq!(
            tags("---\ntags: [writing, \"rust\"]\n---\nText"),
            ["rust", "writing"]
        );
        assert_eq!(
            tags("---\ntitle: x\ntags:\n  - one\n  - two\nauthor: me\n---\n"),
            ["one", "two"]
        );
        assert_eq!(
            tags("---\ntags: markdown, spike\n---\n#idea"),
            ["idea", "markdown", "spike"]
        );
    }
}
