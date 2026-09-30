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
pub struct InlineStyle(u32);

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
    /// A list bullet drawn in place of `-`, `*` or `+`.
    pub const BULLET: Self = Self(1 << 11);
    pub const TASK_OPEN: Self = Self(1 << 12);
    pub const TASK_DONE: Self = Self(1 << 13);
    /// The title that replaces a GitHub alert's `[!KIND]` line.
    pub const ALERT_TITLE: Self = Self(1 << 14);
    /// A quiet label, such as a code block's language.
    pub const LABEL: Self = Self(1 << 15);

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
}

/// When a hidden marker becomes visible again.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reveal {
    /// When the caret or selection touches this range, ends included.
    Touching(Range<usize>),
    /// When the caret or selection is on one of these lines.
    Lines(Range<usize>),
    /// When the caret is inside the marker itself, as for list bullets.
    Inside,
}

/// What a hidden marker shows instead of nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Replacement {
    Bullet,
    Task { checked: bool },
    AlertTitle(Alert),
    Label(String),
}

impl Replacement {
    pub fn text(&self) -> String {
        match self {
            Self::Bullet => "•\u{2002}".to_owned(),
            Self::Task { checked: false } => "☐\u{2002}".to_owned(),
            Self::Task { checked: true } => "☑\u{2002}".to_owned(),
            Self::AlertTitle(alert) => alert.title().to_owned(),
            Self::Label(label) => label.clone(),
        }
    }

    pub const fn style(&self) -> InlineStyle {
        match self {
            Self::Bullet => InlineStyle::BULLET,
            Self::Task { checked: false } => InlineStyle::TASK_OPEN,
            Self::Task { checked: true } => InlineStyle::TASK_DONE,
            Self::AlertTitle(_) => InlineStyle::ALERT_TITLE,
            Self::Label(_) => InlineStyle::LABEL,
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

    /// The innermost link containing `offset`.
    pub fn link_at(&self, offset: usize) -> Option<&Link> {
        self.links
            .iter()
            .filter(|link| link.range.contains(&offset))
            .min_by_key(|link| link.range.len())
    }

    /// The table whose source contains `offset`, ends included.
    pub fn table_at(&self, offset: usize) -> Option<usize> {
        self.tables
            .iter()
            .position(|table| table.range.start <= offset && offset <= table.range.end)
    }
}

const fn options() -> Options {
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
    let mut builder = Builder {
        text,
        prefix_end: (0..lines.len())
            .map(|line| lines.range(line).start)
            .collect(),
        infos: vec![LineInfo::default(); lines.len()],
        lines,
        markers: Vec::new(),
        styles: Vec::new(),
        links: Vec::new(),
        tables: Vec::new(),
        code_blocks: Vec::new(),
        stack: Vec::new(),
    };
    for (event, range) in Parser::new_ext(text, options()).into_offset_iter() {
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

struct Builder<'a> {
    text: &'a str,
    lines: LineIndex,
    /// Where each line's content starts after container prefixes such as `>`.
    prefix_end: Vec<usize>,
    infos: Vec<LineInfo>,
    markers: Vec<Marker>,
    styles: Vec<StyleSpan>,
    links: Vec<Link>,
    tables: Vec<Table>,
    code_blocks: Vec<CodeBlock>,
    stack: Vec<Frame<'a>>,
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
            let bytes = self.text.as_bytes();
            let mut pos = start;
            while pos < line_range.end && matches!(bytes[pos], b' ' | b'\t') {
                pos += 1;
            }
            if pos < line_range.end && bytes[pos] == b'>' {
                pos += 1;
                if pos < line_range.end && bytes[pos] == b' ' {
                    pos += 1;
                }
                self.markers.push(Marker {
                    range: self.prefix_end[line]..pos,
                    reveal: Reveal::Lines(line..line + 1),
                    replacement: None,
                });
                self.prefix_end[line] = pos;
            }
            let info = &mut self.infos[line];
            info.quote_depth = info.quote_depth.saturating_add(1);
            if alert.is_some() {
                info.alert = alert;
            }
        }
        if let Some(alert) = alert {
            let line = lines.start;
            let line_range = self.lines.range(line);
            let start = self.prefix_end[line];
            if self.text[start..line_range.end]
                .trim_start()
                .starts_with("[!")
            {
                self.markers.push(Marker {
                    range: start..line_range.end,
                    reveal: Reveal::Lines(line..line + 1),
                    replacement: Some(Replacement::AlertTitle(alert)),
                });
            }
        }
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
            Event::InlineMath(_) => self.wrap(range, 1, 1, InlineStyle::MATH),
            Event::DisplayMath(_) => self.wrap(range, 2, 2, InlineStyle::MATH),
            Event::InlineHtml(_) | Event::Html(_) => self.style(range.clone(), InlineStyle::HTML),
            Event::FootnoteReference(_) => self.wrap(range, 2, 1, InlineStyle::FOOTNOTE),
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
            Tag::Image { .. } => self.markers_around(&range, content, InlineStyle::IMAGE),
            Tag::Heading { level, .. } => self.heading(&range, content, level),
            Tag::Item => self.item(&range, content, frame.task),
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
        content: Option<Range<usize>>,
        task: Option<(Range<usize>, bool)>,
    ) {
        let bytes = self.text.as_bytes();
        let line_end = self.lines.range(self.lines.line_of(range.start)).end;
        let start = range.start;
        let bullet = matches!(bytes.get(start), Some(b'-' | b'*' | b'+'));
        if let Some((task_range, checked)) = task {
            let mut end = task_range.end;
            while end < line_end && bytes[end] == b' ' {
                end += 1;
            }
            let marker_start = if bullet { start } else { task_range.start };
            self.markers.push(Marker {
                range: marker_start..end,
                reveal: Reveal::Inside,
                replacement: Some(Replacement::Task { checked }),
            });
            if checked && let Some(content) = content {
                let content = end.max(content.start)..content.end;
                self.style(content, InlineStyle::TASK_DONE);
            }
            return;
        }
        if !bullet {
            return;
        }
        let mut end = start + 1;
        let content_start = content.map_or(line_end, |content| content.start.min(line_end));
        while end < content_start && matches!(bytes[end], b' ' | b'\t') {
            end += 1;
        }
        self.markers.push(Marker {
            range: start..end,
            reveal: Reveal::Inside,
            replacement: Some(Replacement::Bullet),
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
        let line_markers = bucket(&self.lines, count, self.markers.iter().map(|m| &m.range));
        let line_styles = bucket(&self.lines, count, self.styles.iter().map(|s| &s.range));
        Analysis {
            lines: self.lines,
            infos: self.infos,
            markers: self.markers,
            styles: self.styles,
            links: self.links,
            tables: self.tables,
            code_blocks: self.code_blocks,
            line_markers,
            line_styles,
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
    fn list_bullets_and_tasks_are_replaced() {
        let text = "- one\n- [x] done\n1. ordered\n";
        let analysis = analyze(text);
        assert_eq!(marker_texts(text, &analysis), ["- ", "- [x] "]);
        assert_eq!(analysis.markers[0].replacement, Some(Replacement::Bullet));
        assert_eq!(
            analysis.markers[1].replacement,
            Some(Replacement::Task { checked: true })
        );
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
    fn quotes_and_alerts() {
        let text = "> [!WARNING]\n> Careful\n";
        let analysis = analyze(text);
        assert_eq!(analysis.infos[0].alert, Some(Alert::Warning));
        assert_eq!(analysis.infos[1].quote_depth, 1);
        assert_eq!(marker_texts(text, &analysis), ["> ", "> ", "[!WARNING]"]);
        assert_eq!(
            analysis.markers[2].replacement,
            Some(Replacement::AlertTitle(Alert::Warning))
        );
    }

    #[test]
    fn nested_quote_markers_stack() {
        let text = "> > deep\n";
        let analysis = analyze(text);
        assert_eq!(analysis.infos[0].quote_depth, 2);
        assert_eq!(marker_texts(text, &analysis), ["> ", "> "]);
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
    fn highlight_and_math() {
        let text = "a ==mark== and $x$\n";
        let analysis = analyze(text);
        assert_eq!(marker_texts(text, &analysis), ["==", "==", "$", "$"]);
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
}
