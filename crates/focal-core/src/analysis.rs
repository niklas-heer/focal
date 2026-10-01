//! Markdown analysis: which byte ranges are styled, which are syntax markers,
//! and what kind of block each line belongs to.
//!
//! The source text is never changed. Everything here is a description of the
//! text that the display layer uses to style it and to hide or replace markers.

use std::ops::{BitOr, BitOrAssign, Range};

use pulldown_cmark::{
    Alignment, BlockQuoteKind, CodeBlockKind, Event, HeadingLevel, LinkType, MetadataBlockKind,
    Options, Parser, Tag,
};

use crate::lines::LineIndex;

/// A set of inline styles applied to a run of text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct InlineStyle(pub(crate) u32);

impl InlineStyle {
    pub const NONE: Self = Self(0);
    pub const STRONG: Self = Self(1);
    pub const EMPHASIS: Self = Self(1 << 1);
    pub const STRIKETHROUGH: Self = Self(1 << 2);
    pub const CODE: Self = Self(1 << 3);
    pub const LINK: Self = Self(1 << 4);
    pub const HIGHLIGHT: Self = Self(1 << 5);
    pub const MATH: Self = Self(1 << 6);
    /// Markdown syntax that is currently revealed.
    pub const MARKER: Self = Self(1 << 7);
    pub const HTML: Self = Self(1 << 8);
    pub const FOOTNOTE: Self = Self(1 << 9);
    pub const IMAGE: Self = Self(1 << 10);
    pub const TASK_DONE: Self = Self(1 << 13);
    /// The title that replaces a GitHub alert's `[!KIND]` line.
    pub const ALERT_TITLE: Self = Self(1 << 14);
    /// A quiet label, such as a code block's language.
    pub const LABEL: Self = Self(1 << 15);
    /// A word the spell checker does not know.
    pub const MISSPELLED: Self = Self(1 << 16);

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0 && other.0 != 0
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl BitOr for InlineStyle {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for InlineStyle {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

/// A GitHub alert kind (`> [!NOTE]` and friends).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Alert {
    Note,
    Tip,
    Important,
    Warning,
    Caution,
}

impl Alert {
    pub const fn title(self) -> &'static str {
        match self {
            Self::Note => "Note",
            Self::Tip => "Tip",
            Self::Important => "Important",
            Self::Warning => "Warning",
            Self::Caution => "Caution",
        }
    }

    /// Its name in HTML class names.
    pub const fn class(self) -> &'static str {
        match self {
            Self::Note => "note",
            Self::Tip => "tip",
            Self::Important => "important",
            Self::Warning => "warning",
            Self::Caution => "caution",
        }
    }

    /// A small symbol before the title, drawn in the alert's color.
    pub const fn icon(self) -> &'static str {
        match self {
            Self::Note => "ⓘ",
            Self::Tip => "✦",
            Self::Important => "!",
            Self::Warning => "⚠",
            Self::Caution => "⊘",
        }
    }

    const fn from_kind(kind: BlockQuoteKind) -> Self {
        match kind {
            BlockQuoteKind::Note => Self::Note,
            BlockQuoteKind::Tip => Self::Tip,
            BlockQuoteKind::Important => Self::Important,
            BlockQuoteKind::Warning => Self::Warning,
            BlockQuoteKind::Caution => Self::Caution,
        }
    }
}

/// The block a line belongs to, which decides its font, size and decoration.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum LineKind {
    #[default]
    Text,
    Heading(u8),
    CodeFence {
        opening: bool,
    },
    Code,
    ThematicBreak,
    FrontMatterFence,
    FrontMatter,
    /// A line of the table with this index in [`Analysis::tables`].
    Table(usize),
    Html,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct LineInfo {
    pub kind: LineKind,
    pub quote_depth: u8,
    pub alert: Option<Alert>,
    /// The code block with this index in [`Analysis::code_blocks`].
    pub code_block: Option<usize>,
    pub prefix: LinePrefix,
}

/// A list item's marker, drawn in the item's marker column.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ListMarker {
    Bullet,
    /// The source label, such as `1.` or `3)`.
    Ordered(String),
    Task {
        checked: bool,
    },
}

/// One container level in front of a line's content, outermost first.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum PrefixLevel {
    Quote(Option<Alert>),
    /// A callout written without `>` (a `:::` container, `>>>`, an MkDocs
    /// admonition): drawn like a quote, with no prefix in the source.
    Block(Option<Alert>),
    /// A list level: the marker on an item's first line, `None` on its other lines.
    List(Option<ListMarker>),
}

/// What stands in front of a line's content. Its source is drawn as elements
/// (quote bars, bullets, checkboxes) and the caret never stops inside it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct LinePrefix {
    pub levels: Vec<PrefixLevel>,
    pub content_start: usize,
    /// The list marker's source on an item's first line, task box and spaces included.
    pub marker: Option<Range<usize>>,
    /// This line's `>` markers, each with one following space.
    pub quotes: Vec<Range<usize>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bias {
    Left,
    Right,
}

/// When a hidden marker becomes visible again.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reveal {
    /// When the caret or selection touches this range, ends included.
    Touching(Range<usize>),
    /// When the caret or selection is on one of these lines.
    Lines(Range<usize>),
}

/// What a hidden marker shows instead of nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Replacement {
    /// A callout's icon and title (empty when the source has its own title).
    Callout(Option<Alert>, String),
    Label(String),
    /// Inline math written in Unicode.
    Math(String),
}

impl Replacement {
    /// A callout title: the icon of `alert` (a quote mark for none), then
    /// `title`.
    pub fn callout(alert: Option<Alert>, title: &str) -> Self {
        let icon = alert.map_or("❝", Alert::icon);
        Self::Callout(alert, format!("{icon} {title}"))
    }

    pub fn text(&self) -> String {
        match self {
            Self::Callout(_, text) | Self::Label(text) | Self::Math(text) => text.clone(),
        }
    }

    pub const fn style(&self) -> InlineStyle {
        match self {
            Self::Callout(..) => InlineStyle::ALERT_TITLE,
            Self::Label(_) => InlineStyle::LABEL,
            Self::Math(_) => InlineStyle::MATH,
        }
    }
}

/// Markdown syntax that is hidden or replaced unless revealed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Marker {
    pub range: Range<usize>,
    pub reveal: Reveal,
    pub replacement: Option<Replacement>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StyleSpan {
    pub range: Range<usize>,
    pub style: InlineStyle,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link {
    /// The whole link, markers included.
    pub range: Range<usize>,
    pub destination: String,
    pub wiki: bool,
}

/// An image: `![alt](destination)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    pub range: Range<usize>,
    pub destination: String,
    pub alt: String,
}

/// A footnote reference (`[^label]`) or the label of its definition
/// (`[^label]:`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Footnote {
    pub label: String,
    /// The reference, or the definition's `[^label]:`.
    pub range: Range<usize>,
    pub definition: bool,
    /// For a definition, its text after the label.
    pub body: Range<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ColumnAlignment {
    None,
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Table {
    pub range: Range<usize>,
    pub lines: Range<usize>,
    pub alignments: Vec<ColumnAlignment>,
    /// Cell content ranges, trimmed. The first row is the header.
    pub rows: Vec<Vec<Range<usize>>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodeBlock {
    pub lines: Range<usize>,
    pub language: Option<String>,
}

/// A marker or style clipped to one line.
#[derive(Clone, Debug)]
pub(crate) struct Piece {
    pub range: Range<usize>,
    pub index: usize,
    /// Whether this is the first line of the item, where a replacement is drawn.
    pub first: bool,
}

/// The analysis of one version of the text.
#[derive(Clone, Debug, Default)]
pub struct Analysis {
    pub lines: LineIndex,
    pub infos: Vec<LineInfo>,
    pub markers: Vec<Marker>,
    pub styles: Vec<StyleSpan>,
    pub links: Vec<Link>,
    pub footnotes: Vec<Footnote>,
    pub images: Vec<Image>,
    /// Display math (`$$ … $$`): its source range and the TeX inside.
    pub display_math: Vec<(Range<usize>, String)>,
    pub tables: Vec<Table>,
    pub code_blocks: Vec<CodeBlock>,
    pub(crate) line_markers: Vec<Vec<Piece>>,
    pub(crate) line_styles: Vec<Vec<Piece>>,
}

impl Analysis {
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    pub fn info(&self, line: usize) -> &LineInfo {
        &self.infos[line.min(self.infos.len() - 1)]
    }

    /// The part of a line after its prefix.
    pub fn content_range(&self, line: usize) -> Range<usize> {
        let range = self.lines.range(line);
        self.info(line)
            .prefix
            .content_start
            .clamp(range.start, range.end)..range.end
    }

    /// Moves an offset out of a line's prefix, where the caret may not stop.
    pub fn snap(&self, offset: usize, bias: Bias) -> usize {
        let line = self.lines.line_of(offset);
        let content = self.content_range(line);
        let line_start = self.lines.range(line).start;
        if offset >= content.start || content.start == line_start {
            return offset;
        }
        if bias == Bias::Left && line > 0 {
            self.lines.range(line - 1).end
        } else {
            content.start
        }
    }

    /// The innermost link containing `offset`.
    pub fn link_at(&self, offset: usize) -> Option<&Link> {
        self.links
            .iter()
            .filter(|link| link.range.contains(&offset))
            .min_by_key(|link| link.range.len())
    }

    /// The footnote reference or definition label containing `offset`.
    pub fn footnote_at(&self, offset: usize) -> Option<&Footnote> {
        self.footnotes
            .iter()
            .find(|note| note.range.start <= offset && offset < note.range.end)
    }

    pub fn footnote_definition(&self, label: &str) -> Option<&Footnote> {
        self.footnotes
            .iter()
            .find(|note| note.definition && note.label == label)
    }

    /// The first reference to the footnote `label`.
    pub fn footnote_reference(&self, label: &str) -> Option<&Footnote> {
        self.footnotes
            .iter()
            .find(|note| !note.definition && note.label == label)
    }

    /// The table whose source contains `offset`, ends included.
    pub fn table_at(&self, offset: usize) -> Option<usize> {
        self.tables
            .iter()
            .position(|table| table.range.start <= offset && offset <= table.range.end)
    }
}

pub(crate) const fn options() -> Options {
    Options::ENABLE_TABLES
        .union(Options::ENABLE_FOOTNOTES)
        .union(Options::ENABLE_STRIKETHROUGH)
        .union(Options::ENABLE_TASKLISTS)
        .union(Options::ENABLE_YAML_STYLE_METADATA_BLOCKS)
        .union(Options::ENABLE_MATH)
        .union(Options::ENABLE_GFM)
        .union(Options::ENABLE_WIKILINKS)
}

/// Analyzes the whole text. Parsing is fast enough to run after every edit.
pub fn analyze(text: &str) -> Analysis {
    let lines = LineIndex::new(text);
    // Other dialects' syntax, rewritten in place; ranges stay those of `text`.
    let shadowed = crate::shadow::shadow(text);
    let mut builder = Builder {
        text,
        shadow: &shadowed,
        prefix_end: (0..lines.len())
            .map(|line| lines.range(line).start)
            .collect(),
        infos: vec![LineInfo::default(); lines.len()],
        lines,
        markers: Vec::new(),
        styles: Vec::new(),
        links: Vec::new(),
        footnotes: Vec::new(),
        images: Vec::new(),
        display_math: Vec::new(),
        tables: Vec::new(),
        code_blocks: Vec::new(),
        stack: Vec::new(),
        containers: Vec::new(),
    };
    for (event, range) in Parser::new_ext(&shadowed, options()).into_offset_iter() {
        builder.event(event, range);
    }
    builder.finish()
}

struct Frame<'a> {
    tag: Tag<'a>,
    range: Range<usize>,
    content: Option<Range<usize>>,
    task: Option<(Range<usize>, bool)>,
    table_row: Vec<Range<usize>>,
}

impl Frame<'_> {
    fn extend(&mut self, range: &Range<usize>) {
        self.content = Some(match self.content.take() {
            Some(content) => content.start.min(range.start)..content.end.max(range.end),
            None => range.clone(),
        });
    }
}

enum ContainerKind {
    Quote(Option<Alert>),
    Item {
        marker: ListMarker,
        marker_range: Range<usize>,
    },
}

struct Container {
    range: Range<usize>,
    kind: ContainerKind,
}

struct Builder<'a> {
    text: &'a str,
    /// The shadow text the parser read (see [`crate::shadow`]): the same
    /// length as `text`, for finding structure such as quote markers.
    shadow: &'a str,
    lines: LineIndex,
    /// Where each line's content starts after container prefixes such as `>`.
    prefix_end: Vec<usize>,
    infos: Vec<LineInfo>,
    markers: Vec<Marker>,
    styles: Vec<StyleSpan>,
    links: Vec<Link>,
    footnotes: Vec<Footnote>,
    images: Vec<Image>,
    display_math: Vec<(Range<usize>, String)>,
    tables: Vec<Table>,
    code_blocks: Vec<CodeBlock>,
    stack: Vec<Frame<'a>>,
    containers: Vec<Container>,
}

impl<'a> Builder<'a> {
    fn event(&mut self, event: Event<'a>, range: Range<usize>) {
        match event {
            Event::Start(tag) => {
                self.start(&tag, &range);
                self.stack.push(Frame {
                    tag,
                    range,
                    content: None,
                    task: None,
                    table_row: Vec::new(),
                });
            }
            Event::End(_) => {
                if let Some(frame) = self.stack.pop() {
                    let range = frame.range.clone();
                    self.end(frame);
                    if let Some(parent) = self.stack.last_mut() {
                        parent.extend(&range);
                    }
                }
            }
            event => {
                self.leaf(&event, &range);
                if let Some(parent) = self.stack.last_mut() {
                    parent.extend(&range);
                }
            }
        }
    }

    fn start(&mut self, tag: &Tag<'a>, range: &Range<usize>) {
        if let Tag::BlockQuote(kind) = tag {
            self.block_quote(range, kind.map(Alert::from_kind));
        }
    }

    fn block_quote(&mut self, range: &Range<usize>, alert: Option<Alert>) {
        let lines = self.lines.lines_of(range);
        for line in lines.clone() {
            let line_range = self.lines.range(line);
            let start = self.prefix_end[line].max(range.start).min(line_range.end);
            let bytes = self.shadow.as_bytes();
            let mut pos = start;
            while pos < line_range.end && matches!(bytes[pos], b' ' | b'\t') {
                pos += 1;
            }
            if pos < line_range.end && bytes[pos] == b'>' {
                let marker = pos;
                pos += 1;
                if pos < line_range.end && bytes[pos] == b' ' {
                    pos += 1;
                }
                // An admonition body's indent the shadow turned into `>   `:
                // the whole indent is prefix.
                if self.text.as_bytes()[marker] != b'>' {
                    while pos < line_range.end && pos - marker < 4 && bytes[pos] == b' ' {
                        pos += 1;
                    }
                }
                self.infos[line]
                    .prefix
                    .quotes
                    .push(self.prefix_end[line]..pos);
                self.prefix_end[line] = pos;
            }
            let info = &mut self.infos[line];
            info.quote_depth = info.quote_depth.saturating_add(1);
            if alert.is_some() {
                info.alert = alert;
            }
        }
        // Titles are added with the other callouts, in `finish`.
        self.containers.push(Container {
            range: self.trim_line_ending(range),
            kind: ContainerKind::Quote(alert),
        });
    }

    fn leaf(&mut self, event: &Event<'a>, range: &Range<usize>) {
        match event {
            Event::Text(_) => self.highlights(range),
            Event::Code(_) => {
                let ticks = self.text[range.clone()]
                    .bytes()
                    .take_while(|&b| b == b'`')
                    .count();
                self.wrap(range, ticks, ticks, InlineStyle::CODE);
            }
            Event::InlineMath(tex) => {
                let (outer, inner) = self.math_delimiters(range, false);
                self.markers_around(&outer, Some(inner.clone()), InlineStyle::MATH);
                // Away from the caret, the TeX reads as Unicode where it can.
                if let Some(text) = crate::texmath::tex_to_unicode(tex)
                    && text != tex.as_ref()
                    && !inner.is_empty()
                {
                    self.markers.push(Marker {
                        range: inner,
                        reveal: Reveal::Touching(outer),
                        replacement: Some(Replacement::Math(text)),
                    });
                }
            }
            Event::DisplayMath(tex) => {
                let (outer, inner) = self.math_delimiters(range, true);
                self.markers_around(&outer, Some(inner), InlineStyle::MATH);
                self.display_math.push((outer, tex.to_string()));
            }
            Event::InlineHtml(_) | Event::Html(_) => self.style(range.clone(), InlineStyle::HTML),
            Event::FootnoteReference(label) => {
                self.wrap(range, 2, 1, InlineStyle::FOOTNOTE);
                self.footnotes.push(Footnote {
                    label: label.to_string(),
                    range: range.clone(),
                    definition: false,
                    body: range.end..range.end,
                });
            }
            Event::Rule => {
                let line = self.lines.line_of(range.start);
                self.infos[line].kind = LineKind::ThematicBreak;
                let line_range = self.lines.range(line);
                self.markers.push(Marker {
                    range: self.prefix_end[line]..line_range.end,
                    reveal: Reveal::Lines(line..line + 1),
                    replacement: None,
                });
            }
            Event::TaskListMarker(checked) => {
                if let Some(item) = self
                    .stack
                    .iter_mut()
                    .rev()
                    .find(|frame| matches!(frame.tag, Tag::Item))
                {
                    item.task = Some((range.clone(), *checked));
                }
            }
            _ => {}
        }
    }

    fn end(&mut self, frame: Frame<'a>) {
        let range = frame.range.clone();
        let content = frame.content.clone();
        match frame.tag {
            Tag::Emphasis => self.markers_around(&range, content, InlineStyle::EMPHASIS),
            Tag::Strong => self.markers_around(&range, content, InlineStyle::STRONG),
            Tag::Strikethrough => self.markers_around(&range, content, InlineStyle::STRIKETHROUGH),
            Tag::Superscript | Tag::Subscript => {
                self.markers_around(&range, content, InlineStyle::NONE);
            }
            Tag::Link {
                link_type,
                dest_url,
                ..
            } => {
                self.markers_around(&range, content, InlineStyle::LINK);
                self.links.push(Link {
                    range,
                    destination: dest_url.to_string(),
                    wiki: matches!(link_type, LinkType::WikiLink { .. }),
                });
            }
            Tag::Image { dest_url, .. } => {
                let alt = content
                    .as_ref()
                    .map_or("", |content| &self.text[content.clone()])
                    .to_owned();
                self.images.push(Image {
                    range: range.clone(),
                    destination: dest_url.to_string(),
                    alt,
                });
                self.markers_around(&range, content, InlineStyle::IMAGE);
            }
            Tag::FootnoteDefinition(label) => {
                let label_end = self.text[range.clone()]
                    .find("]:")
                    .map_or(range.start, |at| range.start + at + 2);
                let body = &self.text[label_end..range.end];
                let start = label_end + (body.len() - body.trim_start().len());
                let end = start + body.trim().len();
                self.style(range.start..label_end, InlineStyle::MARKER);
                self.footnotes.push(Footnote {
                    label: label.to_string(),
                    range: range.start..label_end,
                    definition: true,
                    body: start..end,
                });
            }
            Tag::Heading { level, .. } => self.heading(&range, content, level),
            Tag::Item => self.item(&range, content.as_ref(), frame.task.as_ref()),
            Tag::CodeBlock(kind) => self.code_block(&range, content, &kind),
            Tag::MetadataBlock(MetadataBlockKind::YamlStyle) => self.front_matter(&range),
            Tag::HtmlBlock => {
                for line in self.lines.lines_of(&range) {
                    self.infos[line].kind = LineKind::Html;
                }
            }
            Tag::TableCell => {
                let cell = self.trim_cell(&range);
                if let Some(row) = self
                    .stack
                    .iter_mut()
                    .rev()
                    .find(|f| matches!(f.tag, Tag::TableRow | Tag::TableHead))
                {
                    row.table_row.push(cell);
                }
            }
            Tag::TableRow | Tag::TableHead => {
                if let Some(table) = self
                    .stack
                    .iter_mut()
                    .rev()
                    .find(|f| matches!(f.tag, Tag::Table(_)))
                {
                    // Tables collect their rows in the frame's `table_row` list,
                    // flattened; row boundaries are kept by a sentinel range.
                    table.table_row.extend(frame.table_row);
                    table.table_row.push(usize::MAX..usize::MAX);
                }
            }
            Tag::Table(alignments) => self.table(&range, &alignments, &frame.table_row),
            _ => {}
        }
    }

    /// Adds `open` and `close` byte markers around a leaf's content and styles
    /// the content.
    /// The source extent of math the shadow text found at `range`, and its
    /// content: `$…$`, `$$…$$`, or the `\(…\)`, `\[…\]`, `` $`…`$ `` and
    /// `` ```math `` notations the shadow rewrote.
    fn math_delimiters(&self, range: &Range<usize>, display: bool) -> (Range<usize>, Range<usize>) {
        let text = self.text;
        let width = if display { 2 } else { 1 };
        let (mut start, mut content_start) = (range.start, range.start + width);
        let (mut end, mut content_end) = (range.end, range.end - width);
        let at = |from: usize, len: usize| text.get(from..from + len);
        if display {
            if matches!(at(range.start, 1), Some("`" | "~")) {
                // A math fence: the whole opening and closing fence lines.
                content_start = self.lines.range(self.lines.line_of(range.start)).end;
                let closing = self.lines.range(self.lines.line_of(range.end - 1));
                content_end = closing.start;
                end = closing.start + text[closing.clone()].trim_end().len();
            }
        } else {
            if range.start > 0 && matches!(at(range.start - 1, 2), Some("\\(" | "$`")) {
                start = range.start - 1;
                content_start = range.start + 1;
            }
            if matches!(at(range.end - 1, 2), Some("\\)" | "`$")) {
                end = range.end + 1;
                content_end = range.end - 1;
            }
        }
        (start..end, content_start..content_end.max(content_start))
    }

    fn wrap(&mut self, range: &Range<usize>, open: usize, close: usize, style: InlineStyle) {
        if range.len() < open + close {
            return;
        }
        let content = range.start + open..range.end - close;
        self.markers_around(range, Some(content), style);
    }

    /// Hides everything in `range` outside `content`, revealed when the caret
    /// touches the range.
    fn markers_around(
        &mut self,
        range: &Range<usize>,
        content: Option<Range<usize>>,
        style: InlineStyle,
    ) {
        let Some(content) = content else { return };
        if content.is_empty() {
            return;
        }
        for marker in [range.start..content.start, content.end..range.end] {
            if !marker.is_empty() {
                self.markers.push(Marker {
                    range: marker,
                    reveal: Reveal::Touching(range.clone()),
                    replacement: None,
                });
            }
        }
        if !style.is_empty() {
            self.style(content, style);
        }
    }

    fn style(&mut self, range: Range<usize>, style: InlineStyle) {
        if !range.is_empty() {
            self.styles.push(StyleSpan { range, style });
        }
    }

    /// `==highlight==` is not part of `pulldown-cmark`, so it is found inside
    /// text events. A highlight split across events is not recognized.
    fn highlights(&mut self, range: &Range<usize>) {
        let text = &self.text[range.clone()];
        let mut search = 0;
        while let Some(open) = text[search..].find("==").map(|ix| ix + search) {
            let inner = open + 2;
            let Some(close) = text[inner..].find("==").map(|ix| ix + inner) else {
                break;
            };
            if close > inner && !text[inner..close].contains('\n') {
                let start = range.start + open;
                let end = range.start + close + 2;
                self.wrap(&(start..end), 2, 2, InlineStyle::HIGHLIGHT);
                search = close + 2;
            } else {
                search = inner;
            }
        }
    }

    fn heading(
        &mut self,
        range: &Range<usize>,
        content: Option<Range<usize>>,
        level: HeadingLevel,
    ) {
        let range = self.trim_line_ending(range);
        let lines = self.lines.lines_of(&range);
        for line in lines.clone() {
            self.infos[line].kind = LineKind::Heading(level as u8);
        }
        let content = content.unwrap_or(range.end..range.end);
        for marker in [range.start..content.start, content.end..range.end] {
            if !marker.is_empty() {
                self.markers.push(Marker {
                    range: marker,
                    reveal: Reveal::Lines(lines.clone()),
                    replacement: None,
                });
            }
        }
    }

    fn item(
        &mut self,
        range: &Range<usize>,
        content: Option<&Range<usize>>,
        task: Option<&(Range<usize>, bool)>,
    ) {
        let bytes = self.text.as_bytes();
        let line_end = self.lines.range(self.lines.line_of(range.start)).end;
        let start = range.start;
        let mut end = start;
        while end < line_end && !matches!(bytes[end], b' ' | b'\t') {
            end += 1;
        }
        let label = &self.text[start..end];
        let mut marker = if matches!(label, "-" | "*" | "+") {
            ListMarker::Bullet
        } else {
            ListMarker::Ordered(label.to_owned())
        };
        if let Some((task_range, checked)) = task {
            marker = ListMarker::Task { checked: *checked };
            end = task_range.end;
            if *checked && let Some(content) = content {
                self.style(end.max(content.start)..content.end, InlineStyle::TASK_DONE);
            }
        }
        while end < line_end && matches!(bytes[end], b' ' | b'\t') {
            end += 1;
        }
        self.containers.push(Container {
            range: self.trim_line_ending(range),
            kind: ContainerKind::Item {
                marker,
                marker_range: start..end,
            },
        });
    }

    fn code_block(
        &mut self,
        range: &Range<usize>,
        content: Option<Range<usize>>,
        kind: &CodeBlockKind<'a>,
    ) {
        let range = self.trim_line_ending(range);
        let lines = self.lines.lines_of(&range);
        let index = self.code_blocks.len();
        let language = match kind {
            CodeBlockKind::Fenced(info) => info.split_whitespace().next().map(str::to_owned),
            CodeBlockKind::Indented => None,
        };
        self.code_blocks.push(CodeBlock {
            lines: lines.clone(),
            language: language.clone(),
        });
        for line in lines.clone() {
            self.infos[line].kind = LineKind::Code;
            self.infos[line].code_block = Some(index);
        }
        if matches!(kind, CodeBlockKind::Indented) {
            return;
        }
        let first = lines.start;
        let last = lines.end - 1;
        self.infos[first].kind = LineKind::CodeFence { opening: true };
        self.fence_marker(
            first,
            lines.clone(),
            Some(Replacement::Label(language.unwrap_or_default())),
        );
        let closed = last > first
            && content.is_none_or(|content| content.end <= self.lines.range(last).start);
        if closed {
            self.infos[last].kind = LineKind::CodeFence { opening: false };
            self.fence_marker(last, lines, None);
        }
    }

    fn fence_marker(&mut self, line: usize, block: Range<usize>, replacement: Option<Replacement>) {
        let line_range = self.lines.range(line);
        self.markers.push(Marker {
            range: self.prefix_end[line]..line_range.end,
            reveal: Reveal::Lines(block),
            replacement,
        });
    }

    fn front_matter(&mut self, range: &Range<usize>) {
        let range = self.trim_line_ending(range);
        let lines = self.lines.lines_of(&range);
        for line in lines.clone() {
            self.infos[line].kind = LineKind::FrontMatter;
        }
        for line in [lines.start, lines.end - 1] {
            self.infos[line].kind = LineKind::FrontMatterFence;
            self.fence_marker(line, lines.clone(), None);
        }
    }

    fn table(&mut self, range: &Range<usize>, alignments: &[Alignment], cells: &[Range<usize>]) {
        let range = self.trim_line_ending(range);
        let lines = self.lines.lines_of(&range);
        let index = self.tables.len();
        for line in lines.clone() {
            self.infos[line].kind = LineKind::Table(index);
        }
        let rows = cells
            .split(|cell| cell.start == usize::MAX)
            .filter(|row| !row.is_empty())
            .map(<[Range<usize>]>::to_vec)
            .collect();
        self.tables.push(Table {
            range,
            lines,
            alignments: alignments
                .iter()
                .map(|alignment| match alignment {
                    Alignment::None => ColumnAlignment::None,
                    Alignment::Left => ColumnAlignment::Left,
                    Alignment::Center => ColumnAlignment::Center,
                    Alignment::Right => ColumnAlignment::Right,
                })
                .collect(),
            rows,
        });
    }

    fn trim_cell(&self, range: &Range<usize>) -> Range<usize> {
        let text = &self.text[range.clone()];
        let start = range.start + (text.len() - text.trim_start_matches([' ', '\t', '|']).len());
        let end = range.end - (text.len() - text.trim_end_matches([' ', '\t', '|']).len());
        start..end.max(start)
    }

    fn trim_line_ending(&self, range: &Range<usize>) -> Range<usize> {
        let text = &self.text[range.clone()];
        range.start..range.start + text.trim_end_matches(['\n', '\r']).len()
    }

    fn finish(self) -> Analysis {
        let count = self.lines.len();
        let mut infos = self.infos;
        for (line, info) in infos.iter_mut().enumerate() {
            info.prefix.content_start = self.prefix_end[line];
        }
        let mut containers = self.containers;
        containers.sort_by_key(|c| (c.range.start, std::cmp::Reverse(c.range.end)));
        for container in &containers {
            let lines = self.lines.lines_of(&container.range);
            match &container.kind {
                ContainerKind::Quote(alert) => {
                    for line in lines {
                        infos[line].prefix.levels.push(PrefixLevel::Quote(*alert));
                    }
                }
                ContainerKind::Item {
                    marker,
                    marker_range,
                } => {
                    let first = lines.start;
                    // The item's own indent, measured from where its prefix starts on
                    // its first line; continuation lines skip up to that much whitespace.
                    let own_start = infos[first].prefix.content_start.min(marker_range.start);
                    let indent = marker_range.end - own_start;
                    for line in lines {
                        let prefix = &mut infos[line].prefix;
                        if line == first {
                            prefix.levels.push(PrefixLevel::List(Some(marker.clone())));
                            prefix.marker = Some(marker_range.clone());
                            prefix.content_start = marker_range.end;
                        } else {
                            prefix.levels.push(PrefixLevel::List(None));
                            let end = self.lines.range(line).end;
                            let bytes = self.text.as_bytes();
                            let mut at = prefix.content_start;
                            while at < end
                                && at - prefix.content_start < indent
                                && matches!(bytes[at], b' ' | b'\t')
                            {
                                at += 1;
                            }
                            prefix.content_start = at;
                        }
                    }
                }
            }
        }

        let mut markers = self.markers;
        let mut styles = self.styles;
        apply_callouts(
            self.text,
            &self.lines,
            &mut infos,
            &mut markers,
            &mut styles,
        );
        let line_markers = bucket(&self.lines, count, markers.iter().map(|m| &m.range));
        let line_styles = bucket(&self.lines, count, styles.iter().map(|s| &s.range));
        Analysis {
            lines: self.lines,
            infos,
            markers,
            styles,
            links: self.links,
            footnotes: self.footnotes,
            images: self.images,
            display_math: self.display_math,
            tables: self.tables,
            code_blocks: self.code_blocks,
            line_markers,
            line_styles,
        }
    }
}

/// Draws callouts in every dialect (see [`crate::callouts`]): colors their
/// lines, adds a bar where the source has no `>`, and hides their syntax
/// behind an icon and title.
fn apply_callouts(
    text: &str,
    lines: &LineIndex,
    infos: &mut [LineInfo],
    markers: &mut Vec<Marker>,
    styles: &mut Vec<StyleSpan>,
) {
    use crate::callouts::{Style, callouts};
    let content = |line: usize| {
        let range = lines.range(line);
        range.start..range.start + text[range].trim_end().len()
    };
    for callout in callouts(text) {
        let first = callout.lines.start;
        let alert = callout.alert;
        let title = |written: bool| {
            Some(Replacement::callout(
                alert,
                if written { "" } else { &callout.title },
            ))
        };
        match callout.style {
            Style::Quote => {
                for line in callout.lines.clone() {
                    infos[line].alert = alert;
                    if let Some(level) = infos[line]
                        .prefix
                        .levels
                        .iter_mut()
                        .find(|level| matches!(level, PrefixLevel::Quote(_)))
                    {
                        *level = PrefixLevel::Quote(alert);
                    }
                }
                markers.push(Marker {
                    range: callout.head.clone(),
                    reveal: Reveal::Lines(first..first + 1),
                    replacement: title(callout.title_range.is_some()),
                });
                if let Some(range) = callout.title_range {
                    styles.push(StyleSpan {
                        range,
                        style: InlineStyle::ALERT_TITLE,
                    });
                }
            }
            Style::Fenced { closing } => {
                let colons = text[content(first)].trim_start().starts_with(':');
                if colons && !callout.titled {
                    // A Pandoc div of another kind: its fences stay, quietly.
                    for line in std::iter::once(first).chain(closing) {
                        styles.push(StyleSpan {
                            range: content(line),
                            style: InlineStyle::HTML,
                        });
                    }
                    continue;
                }
                for line in callout.lines.clone() {
                    infos[line].alert = alert;
                    infos[line]
                        .prefix
                        .levels
                        .insert(0, PrefixLevel::Block(alert));
                }
                markers.push(Marker {
                    range: content(first),
                    reveal: Reveal::Lines(first..first + 1),
                    replacement: callout.titled.then(|| title(false)).flatten(),
                });
                if let Some(line) = closing {
                    markers.push(Marker {
                        range: content(line),
                        reveal: Reveal::Lines(line..line + 1),
                        replacement: None,
                    });
                }
            }
            Style::Indented => {
                for line in callout.lines.clone() {
                    infos[line].alert = alert;
                    let levels = &mut infos[line].prefix.levels;
                    match levels.first_mut() {
                        Some(level @ PrefixLevel::Quote(_)) => *level = PrefixLevel::Block(alert),
                        _ => levels.insert(0, PrefixLevel::Block(alert)),
                    }
                }
                markers.push(Marker {
                    range: content(first),
                    reveal: Reveal::Lines(first..first + 1),
                    replacement: title(false),
                });
            }
        }
    }
}

/// Clips each range to the lines it touches.
fn bucket<'r>(
    lines: &LineIndex,
    count: usize,
    ranges: impl Iterator<Item = &'r Range<usize>>,
) -> Vec<Vec<Piece>> {
    let mut buckets = vec![Vec::new(); count];
    for (index, range) in ranges.enumerate() {
        let touched = lines.lines_of(range);
        for line in touched.clone() {
            let line_range = lines.range(line);
            let clipped = range.start.max(line_range.start)..range.end.min(line_range.end);
            if clipped.start < clipped.end || (range.is_empty() && line == touched.start) {
                buckets[line].push(Piece {
                    range: clipped,
                    index,
                    first: line == touched.start,
                });
            }
        }
    }
    buckets
}

#[cfg(test)]
#[allow(clippy::single_range_in_vec_init)] // one marker per line is common here
mod tests {
    use super::*;

    fn marker_texts<'t>(text: &'t str, analysis: &Analysis) -> Vec<&'t str> {
        analysis
            .markers
            .iter()
            .map(|m| &text[m.range.clone()])
            .collect()
    }

    #[test]
    fn inline_markers_surround_content() {
        let text = "a **bold** _em_ `code` ~~gone~~ [link](https://x.y)";
        let analysis = analyze(text);
        assert_eq!(
            marker_texts(text, &analysis),
            [
                "**",
                "**",
                "_",
                "_",
                "`",
                "`",
                "~~",
                "~~",
                "[",
                "](https://x.y)"
            ]
        );
        let styled: Vec<_> = analysis
            .styles
            .iter()
            .map(|s| &text[s.range.clone()])
            .collect();
        assert_eq!(styled, ["bold", "em", "code", "gone", "link"]);
        assert_eq!(analysis.links[0].destination, "https://x.y");
    }

    #[test]
    fn headings_hide_hashes_on_their_lines() {
        let text = "# Title\n\nSetext\n===\n";
        let analysis = analyze(text);
        assert_eq!(analysis.infos[0].kind, LineKind::Heading(1));
        assert_eq!(analysis.infos[2].kind, LineKind::Heading(1));
        assert_eq!(analysis.infos[3].kind, LineKind::Heading(1));
        assert_eq!(marker_texts(text, &analysis), ["# ", "\n==="]);
        assert_eq!(analysis.markers[0].reveal, Reveal::Lines(0..1));
    }

    #[test]
    fn fenced_code_marks_fences_and_body() {
        let text = "```rust\nfn main() {}\n```\n";
        let analysis = analyze(text);
        assert_eq!(
            analysis.infos[0].kind,
            LineKind::CodeFence { opening: true }
        );
        assert_eq!(analysis.infos[1].kind, LineKind::Code);
        assert_eq!(
            analysis.infos[2].kind,
            LineKind::CodeFence { opening: false }
        );
        assert_eq!(analysis.code_blocks[0].language.as_deref(), Some("rust"));
        assert_eq!(
            analysis.markers[0].replacement,
            Some(Replacement::Label("rust".into()))
        );
    }

    #[test]
    fn unclosed_fence_has_no_closing_marker() {
        let analysis = analyze("```\ncode\n");
        assert_eq!(analysis.markers.len(), 1);
        assert_eq!(analysis.infos[1].kind, LineKind::Code);
    }

    #[test]
    fn tables_collect_trimmed_cells() {
        let text = "| a | b |\n|:--|--:|\n| 1 | **2** |\n\nafter\n";
        let analysis = analyze(text);
        let table = &analysis.tables[0];
        assert_eq!(table.lines, 0..3);
        assert_eq!(
            table.alignments,
            [ColumnAlignment::Left, ColumnAlignment::Right]
        );
        let cells: Vec<Vec<&str>> = table
            .rows
            .iter()
            .map(|row| row.iter().map(|c| &text[c.clone()]).collect())
            .collect();
        assert_eq!(cells, [vec!["a", "b"], vec!["1", "**2**"]]);
        assert_eq!(analysis.infos[4].kind, LineKind::Text);
    }

    #[test]
    fn alert_titles_carry_an_icon() {
        assert_eq!(
            Replacement::callout(Some(Alert::Note), "Note").text(),
            "ⓘ Note"
        );
        assert_eq!(
            Replacement::callout(Some(Alert::Warning), "Warning").text(),
            "⚠ Warning"
        );
        assert_eq!(Replacement::callout(None, "Quote").text(), "❝ Quote");
    }

    #[test]
    fn footnotes_pair_references_with_definitions() {
        let text = "One[^a] and two[^b].\n\n[^a]: The note.\n";
        let analysis = analyze(text);
        let reference = analysis.footnote_at(4).unwrap();
        assert_eq!(
            (reference.label.as_str(), reference.definition),
            ("a", false)
        );
        let definition = analysis.footnote_definition("a").unwrap();
        assert_eq!(&text[definition.range.clone()], "[^a]:");
        assert_eq!(&text[definition.body.clone()], "The note.");
        assert!(
            analysis
                .footnote_at(definition.range.start + 1)
                .unwrap()
                .definition
        );
        assert_eq!(
            analysis.footnote_reference("a").map(|r| r.range.start),
            Some(3)
        );
        assert!(
            analysis.footnote_definition("b").is_none(),
            "b has no definition"
        );
    }

    #[test]
    fn highlight_and_math() {
        let text = "a ==mark== and $x$\n";
        let analysis = analyze(text);
        assert_eq!(marker_texts(text, &analysis), ["==", "==", "$", "$"]);
    }

    #[test]
    fn inline_math_reads_as_unicode_away_from_the_caret() {
        use crate::display::{Caret, line_view};
        let text = "Euler: $e^{i\\pi} + 1 = 0$ and $\\begin{x}$.\n";
        let analysis = analyze(text);
        assert_eq!(
            line_view(&analysis, text, 0, None).text,
            "Euler: e^(iπ) + 1 = 0 and \\begin{x}."
        );
        let caret = Caret::new(&analysis, 9..9, 9);
        assert_eq!(
            line_view(&analysis, text, 0, Some(&caret)).text,
            "Euler: $e^{i\\pi} + 1 = 0$ and \\begin{x}."
        );
    }

    #[test]
    fn front_matter_fences() {
        let text = "---\ntitle: x\n---\n\nBody\n";
        let analysis = analyze(text);
        assert_eq!(analysis.infos[0].kind, LineKind::FrontMatterFence);
        assert_eq!(analysis.infos[1].kind, LineKind::FrontMatter);
        assert_eq!(analysis.infos[2].kind, LineKind::FrontMatterFence);
    }

    #[test]
    fn thematic_break_line() {
        let analysis = analyze("a\n\n---\n\nb\n");
        assert_eq!(analysis.infos[2].kind, LineKind::ThematicBreak);
    }

    fn prefix(text: &str, line: usize) -> LinePrefix {
        analyze(text).info(line).prefix.clone()
    }

    #[test]
    fn list_items_have_a_marker_and_content_after_it() {
        let text = "- one\n- [x] done\n12. twelve\n";
        let p = prefix(text, 0);
        assert_eq!(p.levels, [PrefixLevel::List(Some(ListMarker::Bullet))]);
        assert_eq!((p.marker, p.content_start), (Some(0..2), 2));
        let p = prefix(text, 1);
        assert_eq!(
            p.levels,
            [PrefixLevel::List(Some(ListMarker::Task { checked: true }))]
        );
        assert_eq!((p.marker, p.content_start), (Some(6..12), 12));
        let p = prefix(text, 2);
        assert_eq!(
            p.levels,
            [PrefixLevel::List(Some(ListMarker::Ordered("12.".into())))]
        );
        assert_eq!(&text[p.content_start..p.content_start + 6], "twelve");
    }

    #[test]
    fn nested_items_and_continuations_get_columns() {
        // A blank line ends b's paragraph; without it, "  back to a" would be a
        // lazy continuation of b (CommonMark).
        let text = "- a\n  - b\n    more b\n\n  back to a\n";
        let levels = |line| prefix(text, line).levels;
        assert_eq!(
            levels(1),
            [
                PrefixLevel::List(None),
                PrefixLevel::List(Some(ListMarker::Bullet))
            ]
        );
        assert_eq!(
            levels(2),
            [PrefixLevel::List(None), PrefixLevel::List(None)]
        );
        assert_eq!(levels(4), [PrefixLevel::List(None)]);
        assert_eq!(
            &text[prefix(text, 2).content_start..],
            "more b\n\n  back to a\n"
        );
        assert_eq!(&text[prefix(text, 4).content_start..], "back to a\n");
        assert_eq!(prefix(text, 1).marker, Some(6..8));
    }

    #[test]
    fn quotes_and_lists_nest_in_source_order() {
        let text = "> - a\n\n- b\n  > c\n";
        assert_eq!(
            prefix(text, 0).levels,
            [
                PrefixLevel::Quote(None),
                PrefixLevel::List(Some(ListMarker::Bullet))
            ]
        );
        assert_eq!(prefix(text, 0).quotes, [0..2]);
        assert_eq!(prefix(text, 0).content_start, 4);
        assert_eq!(
            prefix(text, 3).levels,
            [PrefixLevel::List(None), PrefixLevel::Quote(None)]
        );
        assert_eq!(&text[prefix(text, 3).content_start..], "c\n");
    }

    #[test]
    fn alerts_keep_their_kind_and_title_marker() {
        let text = "> [!WARNING]\n> Careful\n";
        let analysis = analyze(text);
        assert_eq!(
            analysis.info(1).prefix.levels,
            [PrefixLevel::Quote(Some(Alert::Warning))]
        );
        assert_eq!(analysis.info(0).alert, Some(Alert::Warning));
        assert_eq!(marker_texts(text, &analysis), ["[!WARNING]"]);
        assert_eq!(title_of(&analysis, 0).as_deref(), Some("⚠ Warning"));
    }

    #[test]
    fn crlf_prefix_stops_before_the_line_ending() {
        let text = "- a\r\n- \r\n";
        let p = prefix(text, 1);
        assert_eq!(p.marker, Some(5..7));
        assert_eq!(p.content_start, 7);
        assert_eq!(analyze(text).content_range(1), 7..7);
    }

    #[test]
    fn snapping_moves_out_of_the_prefix() {
        let text = "intro\n- item\n";
        let analysis = analyze(text);
        assert_eq!(analysis.snap(6, Bias::Right), 8);
        assert_eq!(analysis.snap(7, Bias::Right), 8);
        assert_eq!(analysis.snap(7, Bias::Left), 5);
        assert_eq!(analysis.snap(9, Bias::Left), 9);
        assert_eq!(analyze("- x").snap(0, Bias::Left), 2);
    }

    #[test]
    fn parenthesis_math_hides_its_delimiters() {
        let text = r"where \(x^2\) holds";
        let analysis = analyze(text);
        let markers = marker_texts(text, &analysis);
        assert!(
            markers.contains(&r"\(") && markers.contains(&r"\)"),
            "{markers:?}"
        );
        assert!(
            analysis
                .markers
                .iter()
                .any(|m| matches!(&m.replacement, Some(Replacement::Math(t)) if t == "x²")),
            "reads as Unicode"
        );
    }

    #[test]
    fn every_display_math_notation_is_display_math() {
        for (text, tex) in [
            ("\\[\nE = mc^2\n\\]\n", "E = mc^2"),
            ("```math\nx = 1\n```\n", "x = 1"),
            ("$$\ny\n$$\n", "y"),
        ] {
            let analysis = analyze(text);
            assert_eq!(analysis.display_math.len(), 1, "{text:?}");
            let (range, found) = &analysis.display_math[0];
            assert_eq!(found.trim(), tex, "{text:?}");
            assert_eq!(range.start, 0, "{text:?}");
            assert_eq!(
                range.end,
                text.trim_end().len(),
                "the whole fence: {text:?}"
            );
        }
    }

    #[test]
    fn gitlab_inline_math_hides_its_backticks() {
        let text = "Inline $`a^2`$ here";
        let analysis = analyze(text);
        let markers = marker_texts(text, &analysis);
        assert!(
            markers.contains(&"$`") && markers.contains(&"`$"),
            "{markers:?}"
        );
    }

    fn title_of(analysis: &Analysis, line: usize) -> Option<String> {
        analysis.markers.iter().find_map(|m| match &m.replacement {
            Some(Replacement::Callout(_, title))
                if analysis.lines.line_of(m.range.start) == line =>
            {
                Some(title.clone())
            }
            _ => None,
        })
    }

    #[test]
    fn obsidian_callouts_take_their_color_and_title() {
        let text = "> [!info]- My title\n> Body\n";
        let analysis = analyze(text);
        assert_eq!(
            analysis.info(1).prefix.levels,
            [PrefixLevel::Quote(Some(Alert::Note))]
        );
        assert_eq!(analysis.info(0).alert, Some(Alert::Note));
        assert_eq!(marker_texts(text, &analysis), ["[!info]- "]);
        assert_eq!(
            title_of(&analysis, 0).as_deref(),
            Some("ⓘ "),
            "the icon before the written title"
        );
        assert!(
            analysis
                .styles
                .iter()
                .any(|s| s.style.contains(InlineStyle::ALERT_TITLE)
                    && &text[s.range.clone()] == "My title"),
            "the written title is styled as one"
        );
    }

    #[test]
    fn fenced_containers_are_callouts_with_markdown_inside() {
        let text = "::: warning Careful\nSome **bold** text\n:::\n";
        let analysis = analyze(text);
        for line in 0..3 {
            assert_eq!(
                analysis.info(line).prefix.levels.first(),
                Some(&PrefixLevel::Block(Some(Alert::Warning))),
                "line {line}"
            );
        }
        assert_eq!(title_of(&analysis, 0).as_deref(), Some("⚠ Careful"));
        assert!(
            marker_texts(text, &analysis).contains(&":::"),
            "the closing fence hides"
        );
        assert!(
            analysis
                .styles
                .iter()
                .any(|s| s.style.contains(InlineStyle::STRONG) && &text[s.range.clone()] == "bold"),
            "the body is Markdown"
        );
    }

    #[test]
    fn mkdocs_admonitions_read_their_body_as_markdown() {
        let text = "!!! tip \"Try this\"\n    First *one*.\n\n    Second.\n\nAfter\n";
        let analysis = analyze(text);
        for line in 0..4 {
            assert_eq!(
                analysis.info(line).prefix.levels.first(),
                Some(&PrefixLevel::Block(Some(Alert::Tip))),
                "line {line}"
            );
        }
        assert!(analysis.info(5).prefix.levels.is_empty());
        assert_ne!(analysis.info(1).kind, LineKind::Code, "not indented code");
        assert_eq!(&text[analysis.content_range(1)], "First *one*.");
        assert_eq!(title_of(&analysis, 0).as_deref(), Some("✦ Try this"));
    }

    #[test]
    fn gitlab_quotes_are_plain_blocks() {
        let text = ">>>\nQuoted\n>>>\n";
        let analysis = analyze(text);
        assert_eq!(analysis.info(1).prefix.levels, [PrefixLevel::Block(None)]);
        assert_eq!(marker_texts(text, &analysis), [">>>", ">>>"]);
    }
}
