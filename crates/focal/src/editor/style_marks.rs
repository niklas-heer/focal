//! The style tools on a line: parts of speech colored (syntax highlight)
//! and filler words, clichés and redundancies underlined (style check), in
//! prose only.

use std::ops::Range;
use std::rc::Rc;

use focal_core::analysis::LineKind;
use focal_core::display::LineView;

use gpui_kit::component::native_menu::NativeMenu;
use gpui_kit::{Context, Pixels, Point, Window};

use super::{EditKind, Editor, IgnoreStyle, RemoveStyleWord};
use crate::style::{Part, Tagged};

/// Lines whose tags are kept; the cache starts over beyond this.
const TAG_CACHE_LIMIT: usize = 4_000;

impl Editor {
    /// A line's parts of speech and language, cached by its text.
    fn tagged(&self, text: &str) -> Rc<Tagged> {
        let mut cache = self.tag_cache.borrow_mut();
        if cache.len() > TAG_CACHE_LIMIT {
            cache.clear();
        }
        cache
            .entry(text.to_owned())
            .or_insert_with(|| Rc::new(crate::style::tag(text)))
            .clone()
    }

    /// The style check's findings on `line`, as display ranges.
    pub(super) fn style_ranges(
        &self,
        line: usize,
        view: &LineView,
        kind: &LineKind,
    ) -> Vec<Range<usize>> {
        self.style_issues(line, view, kind)
            .into_iter()
            .map(|issue| issue.range)
            .collect()
    }

    pub(super) fn style_issues(
        &self,
        line: usize,
        view: &LineView,
        kind: &LineKind,
    ) -> Vec<focal_core::style::Issue> {
        if !self.style_check
            || !matches!(kind, LineKind::Text | LineKind::Heading(_))
            || view.text.trim().is_empty()
        {
            return Vec::new();
        }
        let german = self.tagged(&view.text).german;
        let markable = self.markable(line, view, false);
        focal_core::style::issues(&view.text, german)
            .into_iter()
            .filter(|issue| markable(&issue.range))
            .filter(|issue| {
                !self
                    .ignored_style
                    .contains(&view.text[issue.range.clone()].to_lowercase())
            })
            .collect()
    }

    /// The words on `line` that syntax highlighting colors, as display
    /// ranges with their parts.
    pub(super) fn part_ranges(
        &self,
        line: usize,
        view: &LineView,
        kind: &LineKind,
    ) -> Vec<(Range<usize>, Part)> {
        if self.syntax_highlight.is_empty()
            || !matches!(kind, LineKind::Text | LineKind::Heading(_))
            || view.text.trim().is_empty()
        {
            return Vec::new();
        }
        let markable = self.markable(line, view, false);
        self.tagged(&view.text)
            .parts
            .iter()
            .filter(|(range, part)| self.syntax_highlight.contains(part) && markable(range))
            .cloned()
            .collect()
    }

    /// The menu for a style finding: what it is, and removing (a filler
    /// word) or ignoring it.
    pub(super) fn style_menu(
        &mut self,
        view: &LineView,
        issue: &focal_core::style::Issue,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = view.map.to_source(issue.range.start)..view.map.to_source(issue.range.end);
        let word = self.text()[range.clone()].to_owned();
        let ignore = Box::new(IgnoreStyle { word: word.clone() });
        let mut menu =
            NativeMenu::new().menu_with_disabled(issue.kind.label(), true, ignore.clone());
        if issue.kind == focal_core::style::IssueKind::Filler {
            menu = menu.separator().menu(
                format!("Remove “{word}”"),
                Box::new(RemoveStyleWord { range, word }),
            );
        } else {
            menu = menu.separator();
        }
        menu.menu("Ignore in This Document", ignore)
            .show(position, window, cx);
    }

    pub(super) fn remove_style_word(
        &mut self,
        action: &RemoveStyleWord,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = action.range.clone();
        if self.text().get(range.clone()) != Some(action.word.as_str()) {
            return;
        }
        // One space goes with the word: the one after it, or else before.
        let text = self.text();
        let range = if text[range.end..].starts_with(' ') {
            range.start..range.end + 1
        } else if text[..range.start].ends_with(' ') {
            range.start - 1..range.end
        } else {
            range
        };
        let at = range.start;
        self.edit(range, "", at..at, EditKind::Other, cx);
    }

    pub(super) fn ignore_style(
        &mut self,
        action: &IgnoreStyle,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ignored_style.insert(action.word.to_lowercase());
        self.refresh();
        cx.notify();
    }

    #[cfg(test)]
    pub(crate) fn style_issues_for_test(&self, line: usize) -> Vec<Range<usize>> {
        let view = &self.snapshot.views[line];
        let kind = &self.snapshot.analysis.info(line).kind;
        self.style_ranges(line, view, kind)
    }

    /// Source ranges, for tests: the display text equals the source here.
    #[cfg(test)]
    pub(crate) fn parts_for_test(&self, line: usize) -> Vec<(Range<usize>, Part)> {
        let view = &self.snapshot.views[line];
        let kind = &self.snapshot.analysis.info(line).kind;
        self.part_ranges(line, view, kind)
    }
}
