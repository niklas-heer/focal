//! The editor's accessibility tree: the rendered text (markers hidden) as
//! AccessKit text runs, tables as table, row and cell nodes, and the caret as
//! a text selection. VoiceOver reads and navigates this instead of the pixels.

use std::cell::{OnceCell, RefCell};
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;

use focal_core::a11y::{TextChunk, offset_of, position_of, text_chunks};
use focal_core::{Analysis, LineView, line_view};
use gpui_kit::A11ySubtreeBuilder;
use gpui_kit::accesskit::{Node, NodeId, Role, TextPosition, TextSelection};

use crate::editor::Row;

/// The text runs of one version of the document, built on first use. Lines
/// are rendered with every marker hidden, so the text a screen reader sees does
/// not change as the caret reveals and hides markers.
pub struct A11yDocument {
    views: Vec<LineView>,
    /// Each line's display text, with its line break except on the last line.
    texts: Vec<String>,
    chunks: Vec<Vec<TextChunk>>,
}

impl A11yDocument {
    fn build(analysis: &Analysis, source: &str) -> Self {
        let views: Vec<LineView> = (0..analysis.line_count())
            .map(|line| line_view(analysis, source, line, None))
            .collect();
        let last = views.len().saturating_sub(1);
        let texts: Vec<String> = views
            .iter()
            .enumerate()
            .map(|(line, view)| {
                let mut text = view.text.clone();
                if line < last {
                    text.push('\n');
                }
                text
            })
            .collect();
        let chunks = texts.iter().map(|text| text_chunks(text)).collect();
        Self {
            views,
            texts,
            chunks,
        }
    }
}

/// Everything the tree is built from, shared cheaply with the render closure.
#[derive(Clone)]
pub struct A11ySource {
    pub analysis: Rc<Analysis>,
    pub text: Rc<str>,
    pub rows: Rc<[Row]>,
    /// Rendered cell texts per table, rows then cells.
    pub table_cells: Rc<[Vec<Vec<String>>]>,
    pub document: Rc<OnceCell<A11yDocument>>,
}

/// Maps text-run node ids back to (line, chunk), for selection requests.
pub type RunIds = Rc<RefCell<HashMap<NodeId, (usize, usize)>>>;

impl A11ySource {
    fn document(&self) -> &A11yDocument {
        self.document
            .get_or_init(|| A11yDocument::build(&self.analysis, &self.text))
    }

    /// Adds the document's nodes under the editor's node and sets the selection.
    pub fn build_tree(
        &self,
        builder: &mut A11ySubtreeBuilder,
        selection: Range<usize>,
        reversed: bool,
        ids: &RunIds,
    ) {
        let started = std::time::Instant::now();
        let document = self.document();
        let mut ours = Vec::new();
        let mut top = Vec::new();
        let mut run_ids = HashMap::new();
        let mut first_run: HashMap<usize, NodeId> = HashMap::new();

        for row in self.rows.iter() {
            match *row {
                Row::Line(line) => {
                    let chunks = &document.chunks[line];
                    if chunks.is_empty() {
                        // An empty last line still needs a run to hold the caret.
                        let id = builder.synthetic_node_id(("run", line, 0usize));
                        // AccessKit expects every text run to have a value.
                        let mut node = Node::new(Role::TextRun);
                        node.set_value("");
                        builder.push_child(id, node);
                        ours.push(id);
                        top.push(id);
                        run_ids.insert(id, (line, 0));
                        first_run.insert(line, id);
                        continue;
                    }
                    for (index, chunk) in chunks.iter().enumerate() {
                        let id = builder.synthetic_node_id(("run", line, index));
                        let mut node = Node::new(Role::TextRun);
                        node.set_value(&document.texts[line][chunk.range.clone()]);
                        node.set_character_lengths(chunk.character_lengths.clone());
                        node.set_word_starts(chunk.word_starts.clone());
                        builder.push_child(id, node);
                        ours.push(id);
                        top.push(id);
                        run_ids.insert(id, (line, index));
                        first_run.entry(line).or_insert(id);
                    }
                }
                Row::Table(table) => {
                    let id = self.table(builder, table, &mut ours);
                    top.push(id);
                }
            }
        }

        let parent = builder.parent_node();
        // Keep children GPUI added for real elements (such as banner buttons).
        let others: Vec<NodeId> = parent
            .children()
            .iter()
            .copied()
            .filter(|id| !ours.contains(id))
            .collect();
        top.extend(others);
        parent.set_children(top);

        let position = |offset: usize| self.position(document, &run_ids, &first_run, offset);
        let (anchor, focus) = if reversed {
            (selection.end, selection.start)
        } else {
            (selection.start, selection.end)
        };
        if let (Some(anchor), Some(focus)) = (position(anchor), position(focus)) {
            parent.set_text_selection(TextSelection { anchor, focus });
        }
        *ids.borrow_mut() = run_ids;
        if std::env::var_os("FOCAL_TRACE").is_some() {
            eprintln!(
                "focal: accessibility tree {} nodes in {:.2} ms",
                ours.len(),
                started.elapsed().as_secs_f64() * 1000.
            );
        }
    }

    fn table(
        &self,
        builder: &mut A11ySubtreeBuilder,
        table: usize,
        ours: &mut Vec<NodeId>,
    ) -> NodeId {
        let rows = self.table_cells.get(table).map_or(&[][..], Vec::as_slice);
        let mut row_ids = Vec::new();
        for (r, cells) in rows.iter().enumerate() {
            let mut cell_ids = Vec::new();
            for (c, cell) in cells.iter().enumerate() {
                let separator = if c + 1 == cells.len() { "\n" } else { "\t" };
                let text = format!("{cell}{separator}");
                let run_id = builder.synthetic_node_id(("cell-run", table, r, c));
                let mut run = Node::new(Role::TextRun);
                if let Some(chunk) = text_chunks(&text).into_iter().next() {
                    run.set_character_lengths(chunk.character_lengths);
                    run.set_word_starts(chunk.word_starts);
                }
                run.set_value(text);
                builder.push_child(run_id, run);
                let cell_id = builder.synthetic_node_id(("cell", table, r, c));
                let mut node = Node::new(if r == 0 {
                    Role::ColumnHeader
                } else {
                    Role::Cell
                });
                // macOS does not expose text runs inside cells; the label is what VoiceOver reads.
                node.set_label(cell.as_str());
                node.set_row_index(r);
                node.set_column_index(c);
                node.set_children(vec![run_id]);
                builder.push_child(cell_id, node);
                ours.extend([run_id, cell_id]);
                cell_ids.push(cell_id);
            }
            let row_id = builder.synthetic_node_id(("row", table, r));
            let mut node = Node::new(Role::Row);
            node.set_row_index(r);
            node.set_children(cell_ids);
            builder.push_child(row_id, node);
            ours.push(row_id);
            row_ids.push(row_id);
        }
        let id = builder.synthetic_node_id(("table", table));
        let mut node = Node::new(Role::Table);
        node.set_row_count(rows.len());
        node.set_column_count(rows.iter().map(Vec::len).max().unwrap_or(0));
        node.set_children(row_ids);
        builder.push_child(id, node);
        ours.push(id);
        id
    }

    fn position(
        &self,
        document: &A11yDocument,
        run_ids: &HashMap<NodeId, (usize, usize)>,
        first_run: &HashMap<usize, NodeId>,
        offset: usize,
    ) -> Option<TextPosition> {
        let line = self.analysis.lines.line_of(offset);
        let display = document.views.get(line)?.map.to_display(offset);
        let (chunk, character_index) = position_of(&document.chunks[line], display);
        let node = run_ids
            .iter()
            .find(|(_, position)| **position == (line, chunk))
            .map(|(&id, _)| id)
            .or_else(|| first_run.get(&line).copied())?;
        Some(TextPosition {
            node,
            character_index,
        })
    }

    /// The source offset of a text position chosen by an assistive technology.
    pub fn source_offset(&self, ids: &RunIds, position: &TextPosition) -> Option<usize> {
        let &(line, chunk) = ids.borrow().get(&position.node)?;
        let document = self.document();
        let display = offset_of(&document.chunks[line], chunk, position.character_index);
        let view = document.views.get(line)?;
        // The line break at the end of a line maps to the end of the line.
        Some(view.map.to_source(display.min(view.text.len())))
    }
}
