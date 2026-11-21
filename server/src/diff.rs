//! Finds what differs between two sequences, so a write can leave the rest
//! untouched.
//!
//! Nothing here knows about Pages or the collaborative document: `align`
//! pairs up the items of two sequences, and `text_hunks` uses it to find the
//! ranges of a text that have to be replaced.

use std::ops::Range;

/// How well an old item and a new one go together. The weight says how much
/// is gained by pairing them and is at least one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fit {
    /// They cannot be paired; the old one goes and the new one is added.
    No,
    /// The old item can be turned into the new one.
    Similar(u32),
    /// The old item already is the new one.
    Same(u32),
}

impl Fit {
    fn weight(self) -> Option<u32> {
        match self {
            Fit::No => None,
            Fit::Similar(weight) | Fit::Same(weight) => Some(weight),
        }
    }
}

/// One step of turning the old sequence into the new one. Steps come in the
/// order of both sequences, and every old and every new item appears once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// The old item at the first index becomes the new item at the second.
    Keep(usize, usize),
    Remove(usize),
    Add(usize),
}

/// Above this many pairs to consider, the part between the common start and
/// end is replaced as a whole instead of being compared item by item.
const MOST_PAIRS: usize = 4_000_000;

/// Pairs the items of an old sequence with those of a new one so that the
/// pairs weigh as much as possible. `fit` judges one old and one new item;
/// `free` names old items that are not part of the content, which may stay
/// unpaired between items that are the same.
pub fn align(
    old: usize,
    new: usize,
    fit: impl Fn(usize, usize) -> Fit,
    free: impl Fn(usize) -> bool,
) -> Vec<Step> {
    let same = |i: usize, j: usize| matches!(fit(i, j), Fit::Same(_));

    // The common start.
    let mut steps = Vec::new();
    let (mut old_start, mut new_start) = (0, 0);
    while new_start < new {
        let mut next = old_start;
        while next < old && free(next) && !same(next, new_start) {
            next += 1;
        }
        if next >= old || !same(next, new_start) {
            break;
        }
        steps.extend((old_start..next).map(Step::Remove));
        steps.push(Step::Keep(next, new_start));
        old_start = next + 1;
        new_start += 1;
    }

    // The common end, collected back to front.
    let mut end = Vec::new();
    let (mut old_end, mut new_end) = (old, new);
    while new_end > new_start {
        let mut next = old_end;
        while next > old_start && free(next - 1) && !same(next - 1, new_end - 1) {
            next -= 1;
        }
        if next <= old_start || !same(next - 1, new_end - 1) {
            break;
        }
        end.extend((next..old_end).rev().map(Step::Remove));
        end.push(Step::Keep(next - 1, new_end - 1));
        old_end = next - 1;
        new_end -= 1;
    }

    // What lies between.
    let (rows, columns) = (old_end - old_start, new_end - new_start);
    if rows == 0 || columns == 0 || rows.saturating_mul(columns) > MOST_PAIRS {
        steps.extend((new_start..new_end).map(Step::Add));
        steps.extend((old_start..old_end).map(Step::Remove));
    } else {
        // best[row][column] is the most weight the items from there on can pair up.
        let width = columns + 1;
        let mut best = vec![0u32; (rows + 1) * width];
        for row in (0..rows).rev() {
            for column in (0..columns).rev() {
                let mut value =
                    best[(row + 1) * width + column].max(best[row * width + column + 1]);
                if let Some(weight) = fit(old_start + row, new_start + column).weight() {
                    value = value.max(weight + best[(row + 1) * width + column + 1]);
                }
                best[row * width + column] = value;
            }
        }
        let (mut row, mut column) = (0, 0);
        while row < rows && column < columns {
            let here = best[row * width + column];
            let paired = fit(old_start + row, new_start + column)
                .weight()
                .is_some_and(|weight| here == weight + best[(row + 1) * width + column + 1]);
            if paired {
                steps.push(Step::Keep(old_start + row, new_start + column));
                row += 1;
                column += 1;
            } else if here == best[row * width + column + 1] {
                // New items go before old ones that stay unpaired.
                steps.push(Step::Add(new_start + column));
                column += 1;
            } else {
                steps.push(Step::Remove(old_start + row));
                row += 1;
            }
        }
        steps.extend((new_start + column..new_end).map(Step::Add));
        steps.extend((old_start + row..old_end).map(Step::Remove));
    }

    steps.extend(end.into_iter().rev());
    steps
}

/// How many characters two texts share at their start and end.
pub fn common_edges(a: &[char], b: &[char]) -> usize {
    let start = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    let end = a[start..]
        .iter()
        .rev()
        .zip(b[start..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    start + end
}

/// A range of the old text that is replaced by a range of the new one.
/// Either may be empty, but not both.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    pub old: Range<usize>,
    pub new: Range<usize>,
}

/// The ranges that turn the old text into the new one, in order. Words are
/// compared as a whole, and within a changed word only the part that differs
/// is replaced.
pub fn text_hunks(old: &[char], new: &[char]) -> Vec<Hunk> {
    let old_tokens = tokens(old);
    let new_tokens = tokens(new);
    let steps = align(
        old_tokens.len(),
        new_tokens.len(),
        |i, j| {
            let (a, b) = (&old[old_tokens[i].clone()], &new[new_tokens[j].clone()]);
            if a == b {
                Fit::Same(a.len() as u32)
            } else {
                Fit::No
            }
        },
        |_| false,
    );

    let mut hunks = Vec::new();
    let mut open = Hunk {
        old: 0..0,
        new: 0..0,
    };
    let mut close = |open: &mut Hunk, old_at: usize, new_at: usize| {
        let (a, b) = (&old[open.old.clone()], &new[open.new.clone()]);
        let start = a.iter().zip(b).take_while(|(x, y)| x == y).count();
        let end = common_edges(a, b) - start;
        let hunk = Hunk {
            old: open.old.start + start..open.old.end - end,
            new: open.new.start + start..open.new.end - end,
        };
        if !hunk.old.is_empty() || !hunk.new.is_empty() {
            hunks.push(hunk);
        }
        *open = Hunk {
            old: old_at..old_at,
            new: new_at..new_at,
        };
    };
    for step in steps {
        match step {
            Step::Keep(i, j) => close(&mut open, old_tokens[i].end, new_tokens[j].end),
            Step::Remove(i) => open.old.end = old_tokens[i].end,
            Step::Add(j) => open.new.end = new_tokens[j].end,
        }
    }
    close(&mut open, old.len(), new.len());
    hunks
}

/// Splits a text into words and single other characters.
fn tokens(text: &[char]) -> Vec<Range<usize>> {
    let mut tokens: Vec<Range<usize>> = Vec::new();
    for (at, character) in text.iter().enumerate() {
        match tokens.last_mut() {
            Some(last) if character.is_alphanumeric() && text[last.start].is_alphanumeric() => {
                last.end = at + 1
            }
            _ => tokens.push(at..at + 1),
        }
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chars(text: &str) -> Vec<char> {
        text.chars().collect()
    }

    /// The parts of both texts that the hunks replace.
    fn replaced(old: &str, new: &str) -> Vec<(String, String)> {
        let (old, new) = (chars(old), chars(new));
        text_hunks(&old, &new)
            .into_iter()
            .map(|hunk| {
                (
                    old[hunk.old].iter().collect(),
                    new[hunk.new].iter().collect(),
                )
            })
            .collect()
    }

    fn pair(old: &str, new: &str) -> (String, String) {
        (old.to_string(), new.to_string())
    }

    #[test]
    fn equal_texts_have_no_hunks() {
        assert_eq!(replaced("the same text", "the same text"), vec![]);
        assert_eq!(replaced("", ""), vec![]);
    }

    #[test]
    fn separate_changes_become_separate_hunks() {
        assert_eq!(
            replaced("The quick brown fox jumps.", "The slow brown fox sleeps."),
            vec![pair("quick", "slow"), pair("jum", "slee")]
        );
    }

    #[test]
    fn only_the_differing_part_of_a_word_is_replaced() {
        assert_eq!(
            replaced("a mispelled word", "a misspelled word"),
            vec![pair("", "s")]
        );
        assert_eq!(replaced("grüße", "grüsse"), vec![pair("ß", "ss")]);
    }

    #[test]
    fn text_is_added_and_removed_at_the_edges() {
        assert_eq!(replaced("", "new"), vec![pair("", "new")]);
        assert_eq!(replaced("old", ""), vec![pair("old", "")]);
        assert_eq!(
            replaced("one two", "one two three"),
            vec![pair("", " three")]
        );
        assert_eq!(replaced("zero one", "one"), vec![pair("zero ", "")]);
    }

    #[test]
    fn free_items_stay_unpaired_between_items_that_are_the_same() {
        // Old item 1 is not part of the content.
        let old = ["a", "", "b"];
        let new = ["a", "b"];
        let steps = align(
            old.len(),
            new.len(),
            |i, j| {
                if old[i] == new[j] {
                    Fit::Same(1)
                } else {
                    Fit::No
                }
            },
            |i| old[i].is_empty(),
        );
        assert_eq!(
            steps,
            vec![Step::Keep(0, 0), Step::Remove(1), Step::Keep(2, 1)]
        );
    }

    #[test]
    fn the_better_pair_wins() {
        let old = ["beta"];
        let new = ["alpha", "bet"];
        let steps = align(
            old.len(),
            new.len(),
            |i, j| Fit::Similar(1 + common_edges(&chars(old[i]), &chars(new[j])) as u32),
            |_| false,
        );
        assert_eq!(steps, vec![Step::Add(0), Step::Keep(0, 1)]);
    }
}
