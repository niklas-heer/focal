use std::ops::Range;

/// Byte offsets of the lines in a text.
///
/// A line's range excludes its line ending, so a trailing `\r` of a CRLF file
/// is never displayed or edited as content.
#[derive(Clone, Debug, Default)]
pub struct LineIndex {
    starts: Vec<usize>,
    ends: Vec<usize>,
}

impl LineIndex {
    pub fn new(text: &str) -> Self {
        let mut starts = vec![0];
        let mut ends = Vec::new();
        for (ix, byte) in text.bytes().enumerate() {
            if byte == b'\n' {
                let end = if ix > 0 && text.as_bytes().get(ix - 1) == Some(&b'\r') {
                    ix - 1
                } else {
                    ix
                };
                ends.push(end);
                starts.push(ix + 1);
            }
        }
        ends.push(text.len());
        Self { starts, ends }
    }

    /// The number of lines. An empty text, or one ending in a newline, has an
    /// empty last line.
    pub fn len(&self) -> usize {
        self.starts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.starts.is_empty()
    }

    /// The content range of `line`, without its line ending.
    pub fn range(&self, line: usize) -> Range<usize> {
        let line = line.min(self.len() - 1);
        self.starts[line]..self.ends[line]
    }

    /// The line containing `offset`. An offset inside a line ending belongs to
    /// the line it ends.
    pub fn line_of(&self, offset: usize) -> usize {
        match self.starts.binary_search(&offset) {
            Ok(line) => line,
            Err(next) => next.saturating_sub(1),
        }
    }

    /// The lines that `range` touches, as a half-open range of line indices.
    pub fn lines_of(&self, range: &Range<usize>) -> Range<usize> {
        let first = self.line_of(range.start);
        let last = if range.end > range.start {
            self.line_of(range.end - 1)
        } else {
            first
        };
        first..last + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_lf_and_crlf() {
        let lines = LineIndex::new("a\r\nbc\nd");
        assert_eq!(lines.len(), 3);
        assert_eq!(lines.range(0), 0..1);
        assert_eq!(lines.range(1), 3..5);
        assert_eq!(lines.range(2), 6..7);
        assert_eq!(lines.line_of(2), 0);
        assert_eq!(lines.line_of(3), 1);
    }

    #[test]
    fn trailing_newline_has_empty_last_line() {
        let lines = LineIndex::new("a\n");
        assert_eq!(lines.len(), 2);
        assert_eq!(lines.range(1), 2..2);
        assert_eq!(LineIndex::new("").len(), 1);
    }
}
