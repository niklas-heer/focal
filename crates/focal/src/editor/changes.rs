//! What changed: after the file changes on disk (an agent's rewrite), the
//! text from before stays as a baseline and the differences are drawn, until
//! they are dismissed; or, on request, the differences from the last commit.

use std::ops::Range;
use std::rc::Rc;
use std::time::{Duration, Instant};

use focal_core::diff;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement as _, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div,
    px,
};

use super::{CompareWithLastCommit, DismissChanges, Editor, NextChange, PreviousChange};
use crate::theme::{MONO_FONT, Theme};

/// How long a diff may take: longer after a reload, short while typing.
const RELOAD_DEADLINE: Duration = Duration::from_millis(50);
const EDIT_DEADLINE: Duration = Duration::from_millis(4);

/// The text the changes are shown against.
pub(crate) enum Baseline {
    /// The text before the file changed on disk; edits since show too.
    Opened(String),
    /// The file in the last commit (`git show HEAD:<file>`).
    Commit(String),
}

impl Baseline {
    fn text(&self) -> &str {
        match self {
            Self::Opened(text) | Self::Commit(text) => text,
        }
    }
}

impl Editor {
    /// Before the file's text is replaced from disk: the text shown until
    /// now becomes the baseline, unless one is kept already, so several
    /// rewrites in a row show together.
    pub(super) fn keep_baseline(&mut self) {
        if self.baseline.is_none() {
            self.baseline = Some(Baseline::Opened(self.text().to_owned()));
        }
    }

    /// The changes against the baseline again, after a reload or an edit.
    pub(super) fn recompute_changes(&mut self, reload: bool) {
        let Some(baseline) = &self.baseline else {
            return;
        };
        let deadline = Instant::now()
            + if reload {
                RELOAD_DEADLINE
            } else {
                EDIT_DEADLINE
            };
        self.changes = Rc::new(diff::changes_until(baseline.text(), self.text(), deadline));
        // A reload that changed nothing (the file saved again as it was)
        // keeps no baseline, so later typing is not marked.
        if reload && self.changes.count() == 0 && matches!(self.baseline, Some(Baseline::Opened(_)))
        {
            self.baseline = None;
        }
    }

    pub(super) fn next_change(&mut self, _: &NextChange, _: &mut Window, cx: &mut Context<Self>) {
        self.step_change(true, cx);
    }

    pub(super) fn previous_change(
        &mut self,
        _: &PreviousChange,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.step_change(false, cx);
    }

    pub(crate) fn step_change(&mut self, forward: bool, cx: &mut Context<Self>) {
        if let Some(ix) = diff::next_hunk(&self.changes.hunks, self.head(), forward) {
            let at = self.changes.hunks[ix].start;
            self.move_to(at, cx);
        }
    }

    pub(super) fn dismiss_changes(
        &mut self,
        _: &DismissChanges,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(self.baseline, Some(Baseline::Opened(_))) {
            self.baseline = None;
            self.changes = Rc::default();
            cx.notify();
        }
    }

    /// View ▸ Changes ▸ Compare with Last Commit: on, the baseline is the
    /// file in the last commit; off, nothing is compared.
    pub(super) fn compare_with_last_commit(
        &mut self,
        _: &CompareWithLastCommit,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(self.baseline, Some(Baseline::Commit(_))) {
            self.baseline = None;
            self.changes = Rc::default();
            cx.notify();
            return;
        }
        let Some(path) = self.document.path.clone() else {
            self.show_error("Save the document to compare it with git.".to_owned(), cx);
            return;
        };
        let committed = cx
            .background_executor()
            .spawn(async move { crate::git::committed_text(&path) });
        cx.spawn(async move |this, cx| {
            let committed = committed.await;
            this.update(cx, |this, cx| {
                match committed {
                    Ok(text) => {
                        this.baseline = Some(Baseline::Commit(text));
                        this.recompute_changes(true);
                    }
                    Err(error) => this.show_error(error, cx),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn comparing_with_commit(&self) -> bool {
        matches!(self.baseline, Some(Baseline::Commit(_)))
    }

    /// Whether changes are shown that Esc may dismiss.
    pub(super) fn dismissable_changes(&self) -> bool {
        matches!(self.baseline, Some(Baseline::Opened(_))) && self.changes.count() > 0
    }

    /// The inserted ranges on the line `range`.
    pub(super) fn inserted_on(&self, line: &Range<usize>) -> Vec<Range<usize>> {
        let inserted = &self.changes.inserted;
        let first = inserted.partition_point(|range| range.end < line.start);
        inserted[first..]
            .iter()
            .take_while(|range| range.start <= line.end)
            .cloned()
            .collect()
    }

    /// Where text was deleted on the line `range`.
    pub(super) fn deleted_on(&self, line: &Range<usize>) -> Vec<usize> {
        self.changes
            .deleted
            .iter()
            .map(|(at, _)| *at)
            .filter(|at| line.start <= *at && *at <= line.end)
            .collect()
    }

    /// The margin bar's color for the line `range`: changed lines are marked
    /// green, lines that only lost text red.
    pub(super) fn change_bar(&self, line: &Range<usize>, theme: &Theme) -> Option<gpui_kit::Hsla> {
        let touches = |range: &Range<usize>| {
            range.start <= line.end && line.start <= range.end.max(range.start)
        };
        if self.changes.inserted.iter().any(touches) {
            Some(theme.changed)
        } else if self
            .changes
            .deleted
            .iter()
            .any(|(at, _)| line.start <= *at && *at <= line.end)
        {
            Some(theme.deleted)
        } else {
            None
        }
    }

    /// The text deleted at `offset`, to show beside the pointer.
    pub(super) fn deleted_at(&self, offset: usize) -> Option<SharedString> {
        let (_, text) = self.changes.deleted.iter().find(|(at, _)| *at == offset)?;
        let mut shown: String = text.trim().chars().take(200).collect();
        if shown.len() < text.trim().len() {
            shown.push('…');
        }
        Some(format!("Deleted: {shown}").into())
    }

    /// The pill at the bottom right while changes show: how many, the way
    /// to the next and previous, and dismissing them.
    pub(super) fn render_changes_pill(
        &self,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let count = self.changes.count();
        if count == 0 {
            return None;
        }
        let commit = self.comparing_with_commit();
        let label = format!(
            "{count} change{}{}",
            if count == 1 { "" } else { "s" },
            if commit { " since last commit" } else { "" }
        );
        let hover = theme.code_background;
        let button = |id: &'static str, text: &'static str, tooltip: &'static str| {
            div()
                .id(id)
                .test_support()
                .aria_label(tooltip)
                .tooltip(move |window, cx| {
                    gpui_kit::component::tooltip::Tooltip::new(tooltip).build(window, cx)
                })
                .px(px(6.))
                .py(px(1.))
                .rounded(px(5.))
                .cursor_pointer()
                .hover(move |style| style.bg(hover))
                .child(text)
        };
        Some(
            div()
                .absolute()
                .right(px(16.))
                .bottom(px(12.))
                .flex()
                .items_center()
                .gap(px(2.))
                .px(px(4.))
                .py(px(3.))
                .rounded(px(8.))
                .bg(theme.surface)
                .border_1()
                .border_color(theme.rule)
                .shadow_md()
                .font_family(MONO_FONT)
                .text_size(px(12.))
                .text_color(theme.text)
                .occlude()
                .child(
                    div()
                        .size(px(7.))
                        .ml(px(4.))
                        .mr(px(2.))
                        .rounded_full()
                        .bg(theme.changed),
                )
                .child(
                    button("changes-count", "", "Next change (⌥⌘↓)")
                        .aria_label(label.clone())
                        .child(label)
                        .on_click(cx.listener(|this, _, _, cx| this.step_change(true, cx))),
                )
                .child(
                    button("changes-previous", "↑", "Previous change (⌥⌘↑)")
                        .on_click(cx.listener(|this, _, _, cx| this.step_change(false, cx))),
                )
                .child(
                    button("changes-next", "↓", "Next change (⌥⌘↓)")
                        .on_click(cx.listener(|this, _, _, cx| this.step_change(true, cx))),
                )
                .when(!commit, |d| {
                    d.child(
                        button("changes-dismiss", "✕", "Dismiss changes (Esc)").on_click(
                            cx.listener(|this, _, window, cx| {
                                this.dismiss_changes(&DismissChanges, window, cx);
                            }),
                        ),
                    )
                })
                .into_any_element(),
        )
    }

    #[cfg(test)]
    pub(crate) fn load_theirs_for_test(&mut self, text: &str, cx: &mut Context<Self>) {
        let format = self.document.format;
        self.load_theirs(text, format, None, cx);
    }

    #[cfg(test)]
    pub(crate) fn changes_for_test(&self) -> &diff::Changes {
        &self.changes
    }
}
