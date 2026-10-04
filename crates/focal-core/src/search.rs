//! `/`, `?`, `n` and `N` for Vim and Helix: the query is typed on the modal
//! line at the bottom of the window, its matches show as it grows, and Enter
//! goes to the one after the cursor. Matches are found as the find bar finds
//! them ([`crate::find`]), so both agree on what a query matches.

use std::ops::Range;

use crate::find;

/// What a key on the open search line asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    /// Show `query`'s matches and go to `target`, the one the cursor would go
    /// to, or back to `origin`, the selection the search started from, when
    /// nothing matches.
    Preview {
        query: String,
        target: Option<Range<usize>>,
        origin: (usize, usize),
    },
    /// Enter: the search is done and the cursor goes to `target`, or stays
    /// at `origin`.
    Accept {
        query: String,
        target: Option<Range<usize>>,
        origin: (usize, usize),
    },
    /// Esc: back to the selection the search started from, `(anchor, head)`.
    Cancel { origin: (usize, usize) },
    /// Nothing to do, as for a key the line has no use for.
    Nothing,
}

/// Where a search starts: the editor's selection, and the offsets matches
/// are looked for after (going forward) and before (going backward).
#[derive(Clone, Copy, Debug)]
pub struct Start {
    pub anchor: usize,
    pub head: usize,
    pub after: usize,
    pub before: usize,
}

#[derive(Debug, Default)]
pub struct Search {
    /// The query being typed, while the line is open.
    line: Option<String>,
    start: Option<Start>,
    /// The last query searched for, which `n` and `N` look for again.
    last: Option<String>,
    backward: bool,
    /// The match the cursor is on, counted from one, and how many there are.
    count: Option<(Option<usize>, usize)>,
}

impl Search {
    /// Opens the search line, searching forward or backward from `start`.
    pub fn open(&mut self, forward: bool, start: Start) {
        self.line = Some(String::new());
        self.start = Some(start);
        self.backward = !forward;
        self.count = None;
    }

    pub const fn is_open(&self) -> bool {
        self.line.is_some()
    }

    /// The search line as shown: `/` or `?`, the query, and how many match.
    pub fn status(&self) -> Option<String> {
        let line = self.line.as_ref()?;
        let prompt = if self.backward { '?' } else { '/' };
        Some(match self.count.filter(|_| !line.is_empty()) {
            Some(count) => format!("{prompt}{line}  {}", count_text(count)),
            None => format!("{prompt}{line}"),
        })
    }

    /// A query searched for elsewhere, as with the find bar, for `n` to
    /// find again.
    pub fn remember(&mut self, query: &str) {
        if !query.is_empty() {
            self.last = Some(query.to_owned());
        }
    }

    /// Sets the last query to `query`, searching in the given direction, as
    /// `*` does.
    pub fn set_last(&mut self, query: String, forward: bool) {
        self.last = Some(query);
        self.backward = !forward;
    }

    pub fn key(&mut self, key: crate::vim::Key, text: &str) -> Step {
        use crate::vim::Key;
        let Some(line) = self.line.as_mut() else {
            return Step::Nothing;
        };
        match key {
            Key::Char(c) => line.push(c),
            Key::Backspace => {
                if line.pop().is_none() {
                    return self.cancel();
                }
            }
            Key::Enter => {
                let typed = self.line.take().unwrap_or_default();
                // An empty query searches for the last one again, as in Vim.
                let query = if typed.is_empty() {
                    self.last.clone().unwrap_or_default()
                } else {
                    typed
                };
                let target = self.preview_target(&query, text);
                let Some(start) = self.start.take() else {
                    return Step::Nothing;
                };
                let origin = (start.anchor, start.head);
                if query.is_empty() {
                    return Step::Cancel { origin };
                }
                self.last = Some(query.clone());
                return Step::Accept {
                    query,
                    target,
                    origin,
                };
            }
            Key::Escape | Key::Ctrl('[' | 'c') => return self.cancel(),
            Key::Tab | Key::Ctrl(_) => return Step::Nothing,
        }
        let query = self.line.clone().unwrap_or_default();
        let target = self.preview_target(&query, text);
        let origin = self
            .start
            .map_or((0, 0), |start| (start.anchor, start.head));
        Step::Preview {
            query,
            target,
            origin,
        }
    }

    fn cancel(&mut self) -> Step {
        self.line = None;
        self.count = None;
        match self.start.take() {
            Some(start) => Step::Cancel {
                origin: (start.anchor, start.head),
            },
            None => Step::Nothing,
        }
    }

    /// The match the open search would go to, counting the matches too.
    fn preview_target(&mut self, query: &str, text: &str) -> Option<Range<usize>> {
        let start = self.start?;
        let (target, count) = step(text, query, !self.backward, start.after, start.before);
        self.count = Some(count);
        target
    }

    /// `n` (or `N`, `reverse`): the last query's next match in the search's
    /// direction, from `after` going forward or `before` going backward.
    /// Returns the query, the match and what to show, or a message when
    /// there is nothing to go to.
    pub fn again(
        &self,
        text: &str,
        reverse: bool,
        after: usize,
        before: usize,
    ) -> Result<(String, Range<usize>, String), String> {
        let query = self.last.clone().ok_or("No previous search")?;
        let forward = self.backward == reverse;
        let (target, count) = step(text, &query, forward, after, before);
        let target = target.ok_or_else(|| format!("Not found: {query}"))?;
        let prompt = if forward { '/' } else { '?' };
        let shown = format!("{prompt}{query}  {}", count_text(count));
        Ok((query, target, shown))
    }
}

/// The match after `after` going forward, or before `before` going backward,
/// wrapping around; and which one it is of how many.
fn step(
    text: &str,
    query: &str,
    forward: bool,
    after: usize,
    before: usize,
) -> (Option<Range<usize>>, (Option<usize>, usize)) {
    let matches = find::find_all(text, query);
    let ix = if forward {
        find::next_match(&matches, after)
    } else {
        find::previous_match(&matches, before)
    };
    (
        ix.map(|ix| matches[ix].clone()),
        (ix.map(|ix| ix + 1), matches.len()),
    )
}

fn count_text(count: (Option<usize>, usize)) -> String {
    match count {
        (_, 0) => "no matches".to_owned(),
        (Some(ix), total) => format!("{ix} of {total}"),
        (None, 1) => "1 match".to_owned(),
        (None, total) => format!("{total} matches"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vim::Key;

    fn start(at: usize) -> Start {
        Start {
            anchor: at,
            head: at,
            after: at + 1,
            before: at,
        }
    }

    fn typed(search: &mut Search, keys: &str, text: &str) -> Step {
        let mut last = Step::Nothing;
        for c in keys.chars() {
            last = search.key(Key::Char(c), text);
        }
        last
    }

    #[test]
    fn typing_previews_the_next_match_and_counts() {
        let text = "two one two two";
        let mut search = Search::default();
        search.open(true, start(0));
        assert_eq!(search.status().as_deref(), Some("/"));
        let step = typed(&mut search, "two", text);
        assert_eq!(
            step,
            Step::Preview {
                query: "two".into(),
                target: Some(8..11),
                origin: (0, 0),
            },
            "the match after the cursor, not the one under it"
        );
        assert_eq!(search.status().as_deref(), Some("/two  2 of 3"));
        typed(&mut search, "x", text);
        assert_eq!(search.status().as_deref(), Some("/twox  no matches"));
    }

    #[test]
    fn enter_accepts_and_n_goes_on() {
        let text = "a b a b a";
        let mut search = Search::default();
        search.open(true, start(0));
        typed(&mut search, "a", text);
        assert_eq!(
            search.key(Key::Enter, text),
            Step::Accept {
                query: "a".into(),
                target: Some(4..5),
                origin: (0, 0),
            }
        );
        assert!(!search.is_open());
        let (_, target, shown) = search.again(text, false, 5, 4).unwrap();
        assert_eq!((target, shown.as_str()), (8..9, "/a  3 of 3"));
        let (_, target, _) = search.again(text, false, 9, 8).unwrap();
        assert_eq!(target, 0..1, "wraps to the top");
        let (_, target, shown) = search.again(text, true, 1, 0).unwrap();
        assert_eq!(
            (target, shown.as_str()),
            (8..9, "?a  3 of 3"),
            "N goes back"
        );
    }

    #[test]
    fn question_mark_searches_backward() {
        let text = "a b a b a";
        let mut search = Search::default();
        search.open(false, start(4));
        typed(&mut search, "a", text);
        let Step::Accept { target, .. } = search.key(Key::Enter, text) else {
            panic!("accepted");
        };
        assert_eq!(target, Some(0..1));
        let (_, target, _) = search.again(text, false, 1, 0).unwrap();
        assert_eq!(target, 8..9, "n keeps going backward, wrapping");
    }

    #[test]
    fn escape_and_emptying_return_to_the_start() {
        let text = "one two";
        let mut search = Search::default();
        search.open(true, start(2));
        typed(&mut search, "tw", text);
        assert_eq!(
            search.key(Key::Escape, text),
            Step::Cancel { origin: (2, 2) }
        );
        assert_eq!(search.status(), None);
        search.open(true, start(2));
        typed(&mut search, "t", text);
        search.key(Key::Backspace, text);
        assert_eq!(
            search.key(Key::Backspace, text),
            Step::Cancel { origin: (2, 2) },
            "backspace on an empty line closes it"
        );
    }

    #[test]
    fn an_empty_query_searches_for_the_last_one() {
        let text = "x y x";
        let mut search = Search::default();
        assert_eq!(
            search.again(text, false, 1, 0).unwrap_err(),
            "No previous search"
        );
        search.remember("x");
        search.open(true, start(0));
        assert_eq!(
            search.key(Key::Enter, text),
            Step::Accept {
                query: "x".into(),
                target: Some(4..5),
                origin: (0, 0),
            }
        );
        assert_eq!(search.again("y", false, 1, 0).unwrap_err(), "Not found: x");
    }
}
