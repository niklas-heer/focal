//! Text encodings and line endings, detected when a file is read and kept
//! when it is written, so a file opens whatever wrote it and saves back the
//! way it was.

/// A file's character encoding.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Encoding {
    #[default]
    Utf8,
    Utf16Le,
    Utf16Be,
    /// Windows' Western European code page, read for any text that is not
    /// UTF-8 (it maps every byte, and covers Latin-1).
    Windows1252,
}

/// How a file's text is stored: its encoding, whether it starts with a byte
/// order mark, and whether its lines end in a lone CR (classic Mac OS).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Format {
    pub encoding: Encoding,
    pub bom: bool,
    pub cr_only: bool,
}

impl Format {
    /// The encoding's name, for the bar; `None` for plain UTF-8.
    pub const fn name(&self) -> Option<&'static str> {
        match self.encoding {
            Encoding::Utf8 if self.bom => Some("UTF-8 with BOM"),
            Encoding::Utf8 => None,
            Encoding::Utf16Le | Encoding::Utf16Be => Some("UTF-16"),
            Encoding::Windows1252 => Some("Windows-1252"),
        }
    }
}

/// The text of `bytes`, with line endings as Focal edits them (LF or CRLF),
/// and how the bytes were stored.
pub fn decode(bytes: &[u8]) -> (String, Format) {
    let (text, encoding, bom) = if let Some(rest) = bytes.strip_prefix(b"\xEF\xBB\xBF") {
        match std::str::from_utf8(rest) {
            Ok(text) => (text.to_owned(), Encoding::Utf8, true),
            Err(_) => (windows_1252(bytes), Encoding::Windows1252, false),
        }
    } else if let Some(rest) = bytes.strip_prefix(b"\xFF\xFE") {
        (
            utf16(rest, u16::from_le_bytes).unwrap_or_default(),
            Encoding::Utf16Le,
            true,
        )
    } else if let Some(rest) = bytes.strip_prefix(b"\xFE\xFF") {
        (
            utf16(rest, u16::from_be_bytes).unwrap_or_default(),
            Encoding::Utf16Be,
            true,
        )
    } else if let Some((text, encoding)) = bare_utf16(bytes) {
        // Before UTF-8: UTF-16 of ASCII text is valid UTF-8 full of NULs.
        (text, encoding, false)
    } else if let Ok(text) = std::str::from_utf8(bytes) {
        (text.to_owned(), Encoding::Utf8, false)
    } else {
        (windows_1252(bytes), Encoding::Windows1252, false)
    };
    // Classic Mac OS ends lines with a lone CR; Focal edits them as LF.
    let cr_only = text.contains('\r') && !text.contains('\n');
    let text = if cr_only {
        text.replace('\r', "\n")
    } else {
        text
    };
    (
        text,
        Format {
            encoding,
            bom,
            cr_only,
        },
    )
}

/// `text` stored as `format`; `None` when the encoding cannot hold one of
/// its characters.
pub fn encode(text: &str, format: &Format) -> Option<Vec<u8>> {
    let text = if format.cr_only {
        std::borrow::Cow::Owned(text.replace('\n', "\r"))
    } else {
        std::borrow::Cow::Borrowed(text)
    };
    let mut bytes = Vec::with_capacity(text.len() + 3);
    match format.encoding {
        Encoding::Utf8 => {
            if format.bom {
                bytes.extend_from_slice(b"\xEF\xBB\xBF");
            }
            bytes.extend_from_slice(text.as_bytes());
        }
        Encoding::Utf16Le => {
            if format.bom {
                bytes.extend_from_slice(b"\xFF\xFE");
            }
            bytes.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
        }
        Encoding::Utf16Be => {
            if format.bom {
                bytes.extend_from_slice(b"\xFE\xFF");
            }
            bytes.extend(text.encode_utf16().flat_map(u16::to_be_bytes));
        }
        Encoding::Windows1252 => {
            let (encoded, _, unmappable) = encoding_rs::WINDOWS_1252.encode(&text);
            if unmappable {
                return None;
            }
            bytes.extend_from_slice(&encoded);
        }
    }
    Some(bytes)
}

fn windows_1252(bytes: &[u8]) -> String {
    encoding_rs::WINDOWS_1252
        .decode_without_bom_handling(bytes)
        .0
        .into_owned()
}

/// `bytes` as UTF-16 code units read with `unit`, if they are valid UTF-16.
fn utf16(bytes: &[u8], unit: fn([u8; 2]) -> u16) -> Option<String> {
    if !bytes.len().is_multiple_of(2) {
        return None;
    }
    let units = bytes.chunks_exact(2).map(|pair| unit([pair[0], pair[1]]));
    char::decode_utf16(units)
        .collect::<Result<String, _>>()
        .ok()
}

/// UTF-16 without a byte order mark, recognized by the zero bytes of its
/// ASCII characters: in every other byte, and never in the rest.
fn bare_utf16(bytes: &[u8]) -> Option<(String, Encoding)> {
    if bytes.len() < 4 || !bytes.len().is_multiple_of(2) {
        return None;
    }
    let pairs = bytes.len() / 2;
    let zeros = |offset: usize| {
        bytes
            .iter()
            .skip(offset)
            .step_by(2)
            .filter(|&&b| b == 0)
            .count()
    };
    let (even, odd) = (zeros(0), zeros(1));
    let (encoding, unit): (Encoding, fn([u8; 2]) -> u16) = if odd * 10 >= pairs * 3 && even == 0 {
        (Encoding::Utf16Le, u16::from_le_bytes)
    } else if even * 10 >= pairs * 3 && odd == 0 {
        (Encoding::Utf16Be, u16::from_be_bytes)
    } else {
        return None;
    };
    utf16(bytes, unit).map(|text| (text, encoding))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(bytes: &[u8]) -> (String, Format) {
        let (text, format) = decode(bytes);
        assert_eq!(
            encode(&text, &format).as_deref(),
            Some(bytes),
            "byte-identical"
        );
        (text, format)
    }

    #[test]
    fn utf8_stays_utf8() {
        let (text, format) = round_trip("# Café\n".as_bytes());
        assert_eq!((text.as_str(), format), ("# Café\n", Format::default()));
    }

    #[test]
    fn a_utf8_byte_order_mark_is_kept_out_of_the_text() {
        let (text, format) = round_trip(b"\xEF\xBB\xBF---\ntitle: x\n---\n");
        assert!(text.starts_with("---"));
        assert_eq!(format.name(), Some("UTF-8 with BOM"));
    }

    #[test]
    fn utf16_with_and_without_a_byte_order_mark() {
        let mut le = vec![0xFF, 0xFE];
        le.extend("Hé ✓\n".encode_utf16().flat_map(u16::to_le_bytes));
        let (text, format) = round_trip(&le);
        assert_eq!(
            (text.as_str(), format.encoding),
            ("Hé ✓\n", Encoding::Utf16Le)
        );
        let mut be = vec![0xFE, 0xFF];
        be.extend("Hi\n".encode_utf16().flat_map(u16::to_be_bytes));
        assert_eq!(round_trip(&be).1.encoding, Encoding::Utf16Be);
        let bare: Vec<u8> = "# Notes\nplain text\n"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        let (text, format) = round_trip(&bare);
        assert_eq!(
            (text.as_str(), format.encoding, format.bom),
            ("# Notes\nplain text\n", Encoding::Utf16Le, false)
        );
    }

    #[test]
    fn other_bytes_read_as_windows_1252() {
        let (text, format) = round_trip(b"caf\xe9 \x93quoted\x94\n");
        assert_eq!(text, "café “quoted”\n");
        assert_eq!(format.name(), Some("Windows-1252"));
    }

    #[test]
    fn edits_keep_the_encoding_unless_it_cannot_hold_them() {
        let (_, format) = decode(b"caf\xe9\n");
        assert_eq!(
            encode("café!\n", &format).as_deref(),
            Some(&b"caf\xe9!\n"[..])
        );
        assert_eq!(
            encode("café ✓\n", &format),
            None,
            "no check mark in Windows-1252"
        );
    }

    #[test]
    fn classic_mac_line_endings_read_as_lines_and_are_kept() {
        let (text, format) = round_trip(b"one\rtwo\r");
        assert_eq!(text, "one\ntwo\n");
        assert!(format.cr_only);
        let (text, format) = round_trip(b"crlf\r\nstays\r\n");
        assert_eq!(
            (text.as_str(), format.cr_only),
            ("crlf\r\nstays\r\n", false)
        );
    }
}
