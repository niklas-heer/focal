//! Fuzzy matching of file paths for the quick switcher.

const WORD_START: i64 = 8;
const CONSECUTIVE: i64 = 5;
const IN_FILE_NAME: i64 = 3;

/// The query is the file name without its extension, or starts it.
const EXACT_NAME: i64 = 20;
const NAME_PREFIX: i64 = 10;
/// Opening a gap between matched letters costs this, plus one per letter.
const GAP: i64 = 2;

/// How well `query` matches `candidate` as a case-insensitive subsequence,
/// higher being better, or `None` when it does not match. Letters at word
/// starts, in runs and in the file name (after the last `/`) score more; gaps
/// between letters cost.
pub fn fuzzy_score(query: &str, candidate: &str) -> Option<i64> {
    let query: Vec<char> = query.chars().flat_map(char::to_lowercase).collect();
    let chars: Vec<char> = candidate.chars().collect();
    if query.is_empty() {
        return Some(0);
    }
    let name_start = chars.iter().rposition(|&c| c == '/').map_or(0, |i| i + 1);
    let bonus = |j: usize| {
        let mut score = 1;
        let starts_word = j == 0
            || matches!(chars[j - 1], '/' | '-' | '_' | ' ' | '.')
            || (chars[j - 1].is_lowercase() && chars[j].is_uppercase());
        if starts_word {
            score += WORD_START;
        }
        if j >= name_start {
            score += IN_FILE_NAME;
        }
        score
    };
    let matches = |i: usize, j: usize| chars[j].to_lowercase().eq(std::iter::once(query[i]));
    let position = |j: usize| i64::try_from(j).unwrap_or(i64::MAX);
    // previous[j]: the best score with the last query letter matched at j.
    let mut previous: Vec<Option<i64>> = vec![None; chars.len()];
    for i in 0..query.len() {
        let mut current = vec![None; chars.len()];
        // The best `score + position` of the previous letter matched before
        // j - 1, so the gap penalty `j - 1 - k + GAP` is one subtraction.
        let mut earlier: Option<i64> = None;
        for j in 0..chars.len() {
            if j >= 2 {
                earlier = earlier.max(previous[j - 2].map(|s| s + position(j - 2)));
            }
            if !matches(i, j) {
                continue;
            }
            current[j] = if i == 0 {
                Some(bonus(j))
            } else {
                let run = j
                    .checked_sub(1)
                    .and_then(|k| previous[k])
                    .map(|s| s + CONSECUTIVE);
                let gap = earlier.map(|s| s - (position(j) - 1) - GAP);
                run.max(gap).map(|s| s + bonus(j))
            };
        }
        previous = current;
    }
    let best = previous.into_iter().flatten().max()?;
    let name: String = chars[name_start..]
        .iter()
        .flat_map(|c| c.to_lowercase())
        .collect();
    let stem = name
        .rsplit_once('.')
        .map_or(name.as_str(), |(stem, _)| stem);
    let query: String = query.iter().collect();
    Some(if stem == query {
        best + EXACT_NAME
    } else if stem.starts_with(&query) {
        best + NAME_PREFIX
    } else {
        best
    })
}

/// The indices of the matching candidates with their scores, best first; ties
/// go to the shorter candidate.
pub fn fuzzy_rank<'a>(
    query: &str,
    candidates: impl IntoIterator<Item = &'a str>,
) -> Vec<(usize, i64)> {
    let mut ranked: Vec<(usize, i64, usize)> = candidates
        .into_iter()
        .enumerate()
        .filter_map(|(ix, c)| fuzzy_score(query, c).map(|score| (ix, score, c.len())))
        .collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.2.cmp(&b.2)).then(a.0.cmp(&b.0)));
    ranked
        .into_iter()
        .map(|(ix, score, _)| (ix, score))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_subsequence_matches_case_insensitively() {
        assert!(fuzzy_score("rdm", "README.md").is_some());
        assert!(fuzzy_score("xyz", "README.md").is_none());
        assert!(
            fuzzy_score("", "a.md").is_some(),
            "an empty query matches all"
        );
    }

    #[test]
    fn file_names_and_word_starts_rank_first() {
        let files = [
            "docs/old/design-notes.md",
            "notes/design.md",
            "desk/sign.md",
        ];
        let ranked = fuzzy_rank("design", files);
        assert_eq!(ranked[0].0, 1, "{ranked:?}");
        assert_eq!(ranked.len(), 3);
        let ranked = fuzzy_rank("dsg", ["a/dsg.md", "design.md"]);
        assert_eq!(ranked[0].0, 0, "consecutive letters beat scattered ones");
    }
}
