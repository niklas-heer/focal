//! What a paste or drop inserts: a link when a URL lands on selected text,
//! Markdown for rich text, and paths for images and files saved or dropped
//! beside the document.

use std::path::Path;

/// One URL and nothing else.
pub fn is_url(text: &str) -> bool {
    let text = text.trim();
    let scheme = ["https://", "http://", "mailto:"]
        .iter()
        .find(|scheme| text.starts_with(*scheme));
    scheme.is_some_and(|scheme| text.len() > scheme.len()) && !text.contains(char::is_whitespace)
}

/// `[label](url)`, with brackets in the label escaped.
pub fn link_over(label: &str, url: &str) -> String {
    let label = label.replace('[', "\\[").replace(']', "\\]");
    format!("[{label}]({})", url.trim())
}

/// Tags that carry Markdown meaning; HTML with none of them is styling
/// only (code editors, terminals) and pastes as its plain text.
const MEANINGFUL: [&str; 17] = [
    "<b>",
    "<b ",
    "<strong",
    "<i>",
    "<i ",
    "<em",
    "<a ",
    "<h1",
    "<h2",
    "<h3",
    "<h4",
    "<ul",
    "<ol",
    "<blockquote",
    "<table",
    "<img",
    "<code",
];

/// Markdown for pasted HTML, or `None` when the plain text should be pasted.
pub fn html_to_markdown(html: &str, plain: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    if !MEANINGFUL.iter().any(|tag| lower.contains(tag)) {
        return None;
    }
    let converter = htmd::HtmlToMarkdown::builder()
        .options(htmd::options::Options {
            heading_style: htmd::options::HeadingStyle::Atx,
            code_block_style: htmd::options::CodeBlockStyle::Fenced,
            code_block_fence: htmd::options::CodeBlockFence::Backticks,
            bullet_list_marker: htmd::options::BulletListMarker::Dash,
            ..Default::default()
        })
        .build();
    let markdown = converter.convert(html).ok()?;
    let markdown = markdown.trim();
    (!markdown.is_empty() && markdown != plain.trim()).then(|| markdown.to_owned())
}

/// A file name for an asset that no file in the folder has yet: the
/// dropped file's own name, or `pasted-<date>-<time>`, with `-2`, `-3` …
pub fn asset_name(
    original: Option<&str>,
    extension: &str,
    (year, month, day, hour, minute, second): (i32, u8, u8, u8, u8, u8),
    taken: impl Fn(&str) -> bool,
) -> String {
    let (stem, extension) = match original.map(|name| name.rsplit_once('.')) {
        Some(Some((stem, extension))) if !stem.is_empty() => (stem.to_owned(), extension),
        Some(_) => (original.unwrap_or_default().to_owned(), extension),
        None => (
            format!("pasted-{year:04}-{month:02}-{day:02}-{hour:02}{minute:02}{second:02}"),
            extension,
        ),
    };
    let mut name = format!("{stem}.{extension}");
    let mut n = 2;
    while taken(&name) {
        name = format!("{stem}-{n}.{extension}");
        n += 1;
    }
    name
}

/// `file` relative to `document_dir`, with `/` and `%20` for spaces (and
/// `%25`, `%28`, `%29` for `%`, `(`, `)`), as Markdown renderers expect.
pub fn relative_link(document_dir: &Path, file: &Path) -> String {
    let from: Vec<_> = document_dir.components().collect();
    let to: Vec<_> = file.components().collect();
    let common = from.iter().zip(&to).take_while(|(a, b)| a == b).count();
    let up = std::iter::repeat_n("..".to_owned(), from.len() - common);
    up.chain(to[common..].iter().map(|part| {
        part.as_os_str()
            .to_string_lossy()
            .replace('%', "%25")
            .replace(' ', "%20")
            .replace('(', "%28")
            .replace(')', "%29")
    }))
    .collect::<Vec<_>>()
    .join("/")
}

/// Image files a drop copies beside the document; other files are linked.
const IMAGE_EXTENSIONS: [&str; 9] = [
    "png", "jpg", "jpeg", "gif", "webp", "svg", "heic", "tiff", "bmp",
];

pub fn is_image(path: &Path) -> bool {
    path.extension().is_some_and(|ext| {
        IMAGE_EXTENSIONS
            .iter()
            .any(|known| ext.eq_ignore_ascii_case(known))
    })
}

/// What dropping `images` and `files` will do, shown while they are dragged
/// over the editor.
pub fn drop_label(images: usize, files: usize, folder: &str) -> Option<String> {
    let plural = |n: usize, word: &str| format!("{n} {word}{}", if n == 1 { "" } else { "s" });
    let place = if folder.is_empty() {
        "beside the document".to_owned()
    } else {
        format!("to {folder}/")
    };
    match (images, files) {
        (0, 0) => None,
        (images, 0) => Some(format!("Add {} {place}", plural(images, "image"))),
        (0, files) => Some(format!("Link {}", plural(files, "file"))),
        (images, files) => Some(format!(
            "Add {} {place} and link {}",
            plural(images, "image"),
            plural(files, "file")
        )),
    }
}

pub fn image_markdown(path: &str) -> String {
    format!("![]({path})")
}

pub fn file_link(name: &str, path: &str) -> String {
    link_over(name, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn urls_are_single_links() {
        assert!(is_url("https://example.com/a?b=c"));
        assert!(is_url("mailto:me@example.com"));
        assert!(!is_url("https://a.com and more"));
        assert!(!is_url("example.com"));
        assert!(!is_url("https://a.com\nhttps://b.com"));
    }

    #[test]
    fn a_url_over_a_selection_makes_a_link() {
        assert_eq!(
            link_over("the docs", "https://a.com"),
            "[the docs](https://a.com)"
        );
        assert_eq!(
            link_over("a [b] c", "https://a.com"),
            "[a \\[b\\] c](https://a.com)"
        );
    }

    #[test]
    fn rich_html_becomes_markdown() {
        let html = "<p>Hello <b>bold</b> and <a href=\"https://a.com\">link</a></p>";
        assert_eq!(
            html_to_markdown(html, "Hello bold and link").as_deref(),
            Some("Hello **bold** and [link](https://a.com)")
        );
    }

    #[test]
    fn code_editor_html_pastes_plain() {
        let html = "<div style=\"color:#000\"><span style=\"color:#00f\">fn</span> main() {}</div>";
        assert_eq!(html_to_markdown(html, "fn main() {}"), None);
    }

    #[test]
    fn asset_names_never_overwrite() {
        let taken = |name: &str| name == "pasted-2026-10-04-101500.png";
        assert_eq!(
            asset_name(None, "png", (2026, 10, 4, 10, 15, 0), taken),
            "pasted-2026-10-04-101500-2.png"
        );
        assert_eq!(
            asset_name(Some("My Photo.JPG"), "jpg", (2026, 10, 4, 0, 0, 0), |_| {
                false
            }),
            "My Photo.JPG"
        );
    }

    #[test]
    fn drops_say_what_they_will_do() {
        assert_eq!(
            drop_label(2, 0, "assets").as_deref(),
            Some("Add 2 images to assets/")
        );
        assert_eq!(
            drop_label(1, 0, "assets").as_deref(),
            Some("Add 1 image to assets/")
        );
        assert_eq!(drop_label(0, 1, "assets").as_deref(), Some("Link 1 file"));
        assert_eq!(
            drop_label(1, 2, "img").as_deref(),
            Some("Add 1 image to img/ and link 2 files")
        );
        assert_eq!(drop_label(0, 0, "assets"), None);
        assert_eq!(
            drop_label(1, 0, "").as_deref(),
            Some("Add 1 image beside the document")
        );
    }

    #[test]
    fn image_files_are_recognized() {
        assert!(is_image(Path::new("a/B.PNG")));
        assert!(is_image(Path::new("x.heic")));
        assert!(!is_image(Path::new("notes.md")));
    }

    #[test]
    fn relative_links_encode_spaces() {
        assert_eq!(
            relative_link(Path::new("/n"), Path::new("/n/assets/My Photo.png")),
            "assets/My%20Photo.png"
        );
        assert_eq!(
            relative_link(Path::new("/n/a"), Path::new("/n/b/c d.md")),
            "../b/c%20d.md",
            "outside the document's folder"
        );
        assert_eq!(image_markdown("assets/a.png"), "![](assets/a.png)");
        assert_eq!(
            file_link("Notes Two", "Notes%20Two.md"),
            "[Notes Two](Notes%20Two.md)"
        );
    }
}
