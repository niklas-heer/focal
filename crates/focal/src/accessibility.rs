//! The editor's accessibility tree: the rendered text (markers hidden) as
//! AccessKit text runs, tables as table, row and cell nodes, and the caret as
//! a text selection. VoiceOver reads and navigates this instead of the pixels.
//!
//! The nodes are built once per version of the text and row layout, only while
//! an assistive app is connected; each frame clones them and sets the selection.

use std::cell::{OnceCell, RefCell};
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::rc::Rc;

use focal_core::a11y::{TextChunk, offset_of, position_of, text_chunks};
use focal_core::{Analysis, LineView, line_view};
use gpui_kit::A11ySubtreeBuilder;
use gpui_kit::accesskit::{Node, NodeId, Role, TextPosition, TextSelection};

use crate::editor::Row;

/// Identifies a synthetic node within the editor: kind and up to three indices.
type NodeKey = (&'static str, usize, usize, usize);

/// A node before it has an id, with the keys of its children.
struct Pending {
    key: NodeKey,
    node: Node,
    children: Vec<NodeKey>,
}

/// The nodes with ids, ready to be cloned into each frame.
struct Resolved {
    /// Identifies the editor node the ids were derived under.
    probe: NodeId,
    nodes: Vec<(NodeId, Node)>,
    ours: HashSet<NodeId>,
    top: Vec<NodeId>,
    runs: HashMap<(usize, usize), NodeId>,
    run_lines: HashMap<NodeId, (usize, usize)>,
}

/// The accessibility nodes of one version of the document. Lines are rendered
/// with every marker hidden, so the text a screen reader sees does not change
/// as the caret reveals and hides markers.
pub struct A11yDocument {
    views: Vec<LineView>,
    /// Byte length of each line's marker text ("• ", "1. ", "☐ ") before its
    /// display text.
    lead: Vec<usize>,
    chunks: Vec<Vec<TextChunk>>,
    pending: Vec<Pending>,
    top: Vec<NodeKey>,
    runs: HashMap<(usize, usize), NodeKey>,
    resolved: RefCell<Option<Rc<Resolved>>>,
}

impl A11yDocument {
    fn build(source: &A11ySource) -> Self {
        let analysis = &source.analysis;
        let views: Vec<LineView> = (0..analysis.line_count())
            .map(|line| line_view(analysis, &source.text, line, None))
            .collect();
        let last = views.len().saturating_sub(1);
        let mut lead = Vec::with_capacity(views.len());
        let texts: Vec<String> = views
            .iter()
            .enumerate()
            .map(|(line, view)| {
                let mut text = crate::prefix::marker_text(&analysis.info(line).prefix);
                lead.push(text.len());
                text.push_str(&view.text);
                if line < last {
                    text.push('\n');
                }
                text
            })
            .collect();
        let chunks: Vec<Vec<TextChunk>> = texts.iter().map(|text| text_chunks(text)).collect();

        let mut document = Self {
            views,
            lead,
            chunks,
            pending: Vec::new(),
            top: Vec::new(),
            runs: HashMap::new(),
            resolved: RefCell::new(None),
        };
        for row in source.rows.iter() {
            match *row {
                Row::Line(line) => document.add_line(line, &texts[line]),
                Row::Table(table) => {
                    let cells = source.table_cells.get(table).map_or(&[][..], Vec::as_slice);
                    document.add_table(table, cells);
                }
            }
        }
        document
    }

    fn push(&mut self, key: NodeKey, node: Node, children: Vec<NodeKey>) {
        self.pending.push(Pending {
            key,
            node,
            children,
        });
    }

    fn add_line(&mut self, line: usize, text: &str) {
        if self.chunks[line].is_empty() {
            // An empty last line still needs a run to hold the caret, and
            // AccessKit expects every text run to have a value.
            let key = ("run", line, 0, 0);
            let mut node = Node::new(Role::TextRun);
            node.set_value("");
            self.push(key, node, Vec::new());
            self.top.push(key);
            self.runs.insert((line, 0), key);
            return;
        }
        for index in 0..self.chunks[line].len() {
            let chunk = &self.chunks[line][index];
            let key = ("run", line, index, 0);
            let mut node = Node::new(Role::TextRun);
            node.set_value(&text[chunk.range.clone()]);
            node.set_character_lengths(chunk.character_lengths.clone());
            node.set_word_starts(chunk.word_starts.clone());
            self.push(key, node, Vec::new());
            self.top.push(key);
            self.runs.insert((line, index), key);
        }
    }

    fn add_table(&mut self, table: usize, rows: &[Vec<String>]) {
        let mut row_keys = Vec::new();
        for (r, cells) in rows.iter().enumerate() {
            let mut cell_keys = Vec::new();
            for (c, cell) in cells.iter().enumerate() {
                let separator = if c + 1 == cells.len() { "\n" } else { "\t" };
                let text = format!("{cell}{separator}");
                let run_key = ("cell-run", table, r, c);
                let mut run = Node::new(Role::TextRun);
                if let Some(chunk) = text_chunks(&text).into_iter().next() {
                    run.set_character_lengths(chunk.character_lengths);
                    run.set_word_starts(chunk.word_starts);
                }
                run.set_value(text);
                self.push(run_key, run, Vec::new());
                let cell_key = ("cell", table, r, c);
                let mut node = Node::new(if r == 0 {
                    Role::ColumnHeader
                } else {
                    Role::Cell
                });
                // macOS does not expose text runs inside cells; the label is what VoiceOver reads.
                node.set_label(cell.as_str());
                node.set_row_index(r);
                node.set_column_index(c);
                self.push(cell_key, node, vec![run_key]);
                cell_keys.push(cell_key);
            }
            let row_key = ("row", table, r, 0);
            let mut node = Node::new(Role::Row);
            node.set_row_index(r);
            self.push(row_key, node, cell_keys);
            row_keys.push(row_key);
        }
        let key = ("table", table, 0, 0);
        let mut node = Node::new(Role::Table);
        node.set_row_count(rows.len());
        node.set_column_count(rows.iter().map(Vec::len).max().unwrap_or(0));
        self.push(key, node, row_keys);
        self.top.push(key);
    }

    /// The nodes with ids derived under this frame's editor node, reused while
    /// that node keeps its identity.
    fn resolve(&self, builder: &A11ySubtreeBuilder) -> Rc<Resolved> {
        let probe = builder.synthetic_node_id(("probe", 0usize, 0usize, 0usize));
        if let Some(resolved) = self.resolved.borrow().as_ref()
            && resolved.probe == probe
        {
            return resolved.clone();
        }
        let id = |key: &NodeKey| builder.synthetic_node_id(key);
        let nodes: Vec<(NodeId, Node)> = self
            .pending
            .iter()
            .map(|pending| {
                let mut node = pending.node.clone();
                if !pending.children.is_empty() {
                    node.set_children(pending.children.iter().map(id).collect::<Vec<_>>());
                }
                (id(&pending.key), node)
            })
            .collect();
        let runs: HashMap<(usize, usize), NodeId> = self
            .runs
            .iter()
            .map(|(&position, key)| (position, id(key)))
            .collect();
        let run_lines = runs.iter().map(|(&position, &id)| (id, position)).collect();
        let ours = nodes.iter().map(|(id, _)| *id).collect();
        let resolved = Rc::new(Resolved {
            probe,
            nodes,
            ours,
            top: self.top.iter().map(id).collect(),
            runs,
            run_lines,
        });
        *self.resolved.borrow_mut() = Some(resolved.clone());
        resolved
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
        self.document.get_or_init(|| A11yDocument::build(self))
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
        let resolved = document.resolve(builder);
        for (id, node) in &resolved.nodes {
            builder.push_child(*id, node.clone());
        }

        let parent = builder.parent_node();
        // `push_child` made every node a child of the editor node; keep only the
        // top-level ones, in reading order, plus children GPUI added for real
        // elements (such as banner buttons).
        let others: Vec<NodeId> = parent
            .children()
            .iter()
            .copied()
            .filter(|id| !resolved.ours.contains(id))
            .collect();
        let mut children = resolved.top.clone();
        children.extend(others);
        parent.set_children(children);

        let position = |offset: usize| self.position(document, &resolved, offset);
        let (anchor, focus) = if reversed {
            (selection.end, selection.start)
        } else {
            (selection.start, selection.end)
        };
        if let (Some(anchor), Some(focus)) = (position(anchor), position(focus)) {
            parent.set_text_selection(TextSelection { anchor, focus });
        }
        ids.borrow_mut().clone_from(&resolved.run_lines);
        if std::env::var_os("FOCAL_TRACE").is_some() {
            eprintln!(
                "focal: accessibility tree {} nodes in {:.2} ms",
                resolved.nodes.len(),
                started.elapsed().as_secs_f64() * 1000.
            );
        }
    }

    fn position(
        &self,
        document: &A11yDocument,
        resolved: &Resolved,
        offset: usize,
    ) -> Option<TextPosition> {
        let line = self.analysis.lines.line_of(offset);
        let display = document.views.get(line)?.map.to_display(offset) + document.lead[line];
        let (chunk, character_index) = position_of(&document.chunks[line], display);
        let node = *resolved
            .runs
            .get(&(line, chunk))
            .or_else(|| resolved.runs.get(&(line, 0)))?;
        Some(TextPosition {
            node,
            character_index,
        })
    }

    /// The source offset of a text position chosen by an assistive technology.
    pub fn source_offset(&self, ids: &RunIds, position: &TextPosition) -> Option<usize> {
        let &(line, chunk) = ids.borrow().get(&position.node)?;
        let document = self.document();
        let display = offset_of(&document.chunks[line], chunk, position.character_index)
            .saturating_sub(document.lead[line]);
        let view = document.views.get(line)?;
        // The line break at the end of a line maps to the end of the line.
        Some(view.map.to_source(display.min(view.text.len())))
    }
}
