//! A Markdown table as an editable grid, and its aligned serialization.

use std::ops::Range;

use unicode_width::UnicodeWidthStr as _;

use crate::analysis::{Analysis, ColumnAlignment};
use crate::editing::Change;

/// Delimiter rows need at least three dashes (including alignment colons).
const MIN_WIDTH: usize = 3;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableModel {
    pub alignments: Vec<ColumnAlignment>,
    /// `rows[0]` is the header. Cells are Markdown source with `\|` unescaped.
    pub rows: Vec<Vec<String>>,
}

fn unescape(cell: &str) -> String {
    cell.replace("\\|", "|")
}

fn escape(cell: &str) -> String {
    cell.replace('|', "\\|")
}

impl TableModel {
    pub fn from_analysis(analysis: &Analysis, text: &str, table: usize) -> Option<Self> {
        let table = analysis.tables.get(table)?;
        let columns = table.alignments.len().max(1);
        let rows = table
            .rows
            .iter()
            .map(|row| {
                (0..columns)
                    .map(|c| {
                        row.get(c)
                            .map_or_else(String::new, |cell| unescape(&text[cell.clone()]))
                    })
                    .collect()
            })
            .collect();
        let mut alignments = table.alignments.clone();
        alignments.resize(columns, ColumnAlignment::None);
        Some(Self { alignments, rows })
    }

    pub fn column_count(&self) -> usize {
        self.alignments.len()
    }

    pub fn cell(&self, row: usize, column: usize) -> &str {
        self.rows
            .get(row)
            .and_then(|r| r.get(column))
            .map_or("", String::as_str)
    }

    pub fn serialize(&self, prefix: &str, line_ending: &str) -> String {
        let escaped: Vec<Vec<String>> = self
            .rows
            .iter()
            .map(|row| row.iter().map(|c| escape(c)).collect())
            .collect();
        let widths: Vec<usize> = (0..self.column_count())
            .map(|c| {
                escaped
                    .iter()
                    .map(|row| row[c].width())
                    .max()
                    .unwrap_or(0)
                    .max(MIN_WIDTH)
            })
            .collect();
        let line = |cells: Vec<String>| format!("| {} |", cells.join(" | "));
        let pad = |cell: &str, c: usize| {
            let space = widths[c] - cell.width();
            match self.alignments[c] {
                ColumnAlignment::Right => format!("{}{cell}", " ".repeat(space)),
                ColumnAlignment::Center => {
                    let left = space / 2;
                    format!("{}{cell}{}", " ".repeat(left), " ".repeat(space - left))
                }
                ColumnAlignment::None | ColumnAlignment::Left => {
                    format!("{cell}{}", " ".repeat(space))
                }
            }
        };
        let delimiter = |c: usize| {
            let w = widths[c];
            match self.alignments[c] {
                ColumnAlignment::None => "-".repeat(w),
                ColumnAlignment::Left => format!(":{}", "-".repeat(w - 1)),
                ColumnAlignment::Right => format!("{}:", "-".repeat(w - 1)),
                ColumnAlignment::Center => format!(":{}:", "-".repeat(w - 2)),
            }
        };
        let mut lines = Vec::with_capacity(escaped.len() + 1);
        for (r, row) in escaped.iter().enumerate() {
            lines.push(line(
                row.iter()
                    .enumerate()
                    .map(|(c, cell)| pad(cell, c))
                    .collect(),
            ));
            if r == 0 {
                lines.push(line((0..self.column_count()).map(delimiter).collect()));
            }
        }
        lines.join(&format!("{line_ending}{prefix}"))
    }

    pub fn replace(&self, analysis: &Analysis, text: &str, table: usize) -> Option<Change> {
        let range: Range<usize> = analysis.tables.get(table)?.range.clone();
        let first_line = analysis.lines.range(analysis.lines.line_of(range.start));
        let prefix = &text[first_line.start..range.start];
        let line_ending = if text[range.clone()].contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        };
        Some(Change {
            selection: range.start..range.start,
            text: self.serialize(prefix, line_ending),
            range,
        })
    }

    /// Sets a cell's text. A table row is one line, so line breaks become
    /// spaces.
    pub fn set_cell(&mut self, row: usize, column: usize, text: &str) {
        if let Some(cell) = self.rows.get_mut(row).and_then(|r| r.get_mut(column)) {
            *cell = text.replace("\r\n", " ").replace(['\n', '\r'], " ");
        }
    }

    pub fn insert_row(&mut self, at: usize) {
        let at = at.clamp(1, self.rows.len());
        self.rows
            .insert(at, vec![String::new(); self.column_count()]);
    }

    pub fn delete_row(&mut self, row: usize) {
        if row > 0 && row < self.rows.len() {
            self.rows.remove(row);
        }
    }

    pub fn move_row(&mut self, from: usize, to: usize) {
        if from > 0 && to > 0 && from < self.rows.len() && to < self.rows.len() {
            let row = self.rows.remove(from);
            self.rows.insert(to, row);
        }
    }

    pub fn insert_column(&mut self, at: usize) {
        let at = at.min(self.column_count());
        self.alignments.insert(at, ColumnAlignment::None);
        for row in &mut self.rows {
            row.insert(at, String::new());
        }
    }

    pub fn delete_column(&mut self, column: usize) {
        if self.column_count() > 1 && column < self.column_count() {
            self.alignments.remove(column);
            for row in &mut self.rows {
                row.remove(column);
            }
        }
    }

    pub fn move_column(&mut self, from: usize, to: usize) {
        let count = self.column_count();
        if from < count && to < count {
            let alignment = self.alignments.remove(from);
            self.alignments.insert(to, alignment);
            for row in &mut self.rows {
                let cell = row.remove(from);
                row.insert(to, cell);
            }
        }
    }

    pub fn set_alignment(&mut self, column: usize, alignment: ColumnAlignment) {
        if let Some(slot) = self.alignments.get_mut(column) {
            *slot = alignment;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::analyze;

    fn model(text: &str) -> TableModel {
        TableModel::from_analysis(&analyze(text), text, 0).unwrap()
    }

    #[test]
    fn a_line_break_in_a_cell_becomes_a_space() {
        let mut m = model("| a |\n|---|\n| 1 |\n");
        m.set_cell(1, 0, "one\r\ntwo\nthree");
        assert_eq!(m.cell(1, 0), "one two three");
    }

    #[test]
    fn reads_cells_and_alignments() {
        let m = model("| a | b | c |\n|:--|:-:|--:|\n| 1 | `x\\|y` |\n");
        assert_eq!(
            m.alignments,
            [
                ColumnAlignment::Left,
                ColumnAlignment::Center,
                ColumnAlignment::Right
            ]
        );
        assert_eq!(m.rows, [vec!["a", "b", "c"], vec!["1", "`x|y`", ""]]);
    }

    #[test]
    fn serializes_aligned_escaped_and_wide() {
        let m = model("| a | b |\n|:-|-:|\n| 日本 | x\\|y |\n");
        assert_eq!(
            m.serialize("", "\n"),
            "| a    |    b |\n| :--- | ---: |\n| 日本 | x\\|y |"
        );
    }

    #[test]
    fn centered_and_unaligned_columns() {
        let m = model("| name | n |\n|:---:|---|\n| x | 1 |\n");
        assert_eq!(
            m.serialize("", "\n"),
            "| name | n   |\n| :--: | --- |\n|  x   | 1   |"
        );
    }

    #[test]
    fn serialized_tables_parse_back_to_the_same_model() {
        let m = model("| a | b |\n|---|--:|\n| **x** | `p\\|q` |\n| | y |\n");
        let source = m.serialize("", "\n");
        assert_eq!(model(&source), m);
    }

    #[test]
    fn replacing_keeps_the_quote_prefix_and_crlf() {
        let text = "> | a | b |\r\n> |---|---|\r\n> | 1 | 2 |\r\n\r\nafter\r\n";
        let analysis = analyze(text);
        let mut m = TableModel::from_analysis(&analysis, text, 0).unwrap();
        m.set_cell(1, 1, "22");
        let change = m.replace(&analysis, text, 0).unwrap();
        let mut out = text.to_owned();
        out.replace_range(change.range.clone(), &change.text);
        assert_eq!(
            out,
            "> | a   | b   |\r\n> | --- | --- |\r\n> | 1   | 22  |\r\n\r\nafter\r\n"
        );
    }

    #[test]
    fn row_and_column_operations() {
        let mut m = model("| a | b |\n|---|---|\n| 1 | 2 |\n| 3 | 4 |\n");
        m.insert_row(2);
        assert_eq!(m.rows[2], ["", ""]);
        m.delete_row(2);
        m.delete_row(0);
        assert_eq!(m.rows.len(), 3, "the header stays");
        m.move_row(2, 1);
        assert_eq!(m.rows[1], ["3", "4"]);
        m.move_row(1, 0);
        assert_eq!(
            m.rows[0],
            ["a", "b"],
            "body rows cannot move above the header"
        );
        m.insert_column(1);
        assert_eq!(m.rows[0], ["a", "", "b"]);
        assert_eq!(m.alignments.len(), 3);
        m.move_column(0, 2);
        assert_eq!(m.rows[0], ["", "b", "a"]);
        m.set_alignment(2, ColumnAlignment::Right);
        assert_eq!(m.alignments[2], ColumnAlignment::Right);
        m.delete_column(0);
        m.delete_column(0);
        m.delete_column(0);
        assert_eq!(m.column_count(), 1, "at least one column stays");
    }
}
