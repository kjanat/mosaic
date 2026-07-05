//! Conservative nearest-match selection for "did you mean" diagnostics.
//!
//! Several passes answer the same question: the user wrote an identifier
//! that matches nothing — is one known name a plausible near-miss worth
//! offering as a fix? This module owns the shared edit-distance metric and
//! the selection rule; callers supply their own candidate sets (reference
//! labels, citation keys, `#set` targets, directive keyword arguments).

/// Byte-level edit distance counting an adjacent transposition as one edit
/// (optimal string alignment, the restricted Damerau-Levenshtein variant).
///
/// Callers only pass identifier-alphabet names (directive targets, kwarg
/// keys, reference labels, citation keys), all ASCII, so byte distance
/// equals character distance and case differences count as real edits.
/// Charging a swapped pair one edit instead of two matters at the
/// conservative [`nearest_match`] bound: `wdith` → `width` is a single
/// transposition, while two substitutions would push it past the `len / 3`
/// threshold for a five-byte key.
///
/// Storage is three reusable rows — current, previous, and the row before
/// that (the transposition rule reaches two rows back): `curr[j]` holds the
/// distance from the processed prefix of `a` to `b[..j]`.
pub(crate) fn edit_distance(a: &str, b: &str) -> usize {
    let a = a.as_bytes();
    let b = b.as_bytes();
    let mut prev2: Vec<usize> = vec![0; b.len() + 1];
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr: Vec<usize> = vec![0; b.len() + 1];
    for (i, &ai) in a.iter().enumerate() {
        curr[0] = i + 1;
        for (j, &bj) in b.iter().enumerate() {
            let cost = usize::from(ai != bj);
            let mut best = (prev[j] + cost) // substitute (or keep on match)
                .min(prev[j + 1] + 1) // delete from `a`
                .min(curr[j] + 1); // insert into `a`
            if i > 0 && j > 0 && ai == b[j - 1] && a[i - 1] == bj {
                best = best.min(prev2[j - 1] + 1); // transpose adjacent pair
            }
            curr[j + 1] = best;
        }
        // Rotate: current becomes previous, previous becomes two-back.
        std::mem::swap(&mut prev2, &mut prev);
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b.len()]
}

/// The single candidate that is a reasonable near-miss for `unknown`, if any.
///
/// "Reasonable" is deliberately conservative — a wrong guess is worse than
/// no guess. The rule mirrors the citation-key heuristic in
/// [`crate::bibliography`]; `nearest_label` in [`crate::resolve`] shares the
/// length floor and distance bound but keeps its own deterministic
/// lexicographic tie-break instead of the tie rule below:
///
/// - names shorter than three bytes get no suggestion (a one-edit guess on
///   a one- or two-byte name is noise, not help);
/// - the edit distance must be within `unknown.len() / 3`: rustc's "did you
///   mean" style bound. With the length floor that bound is always at least
///   1, admitting `tex` → `text` (distance 1, bound 1) while rejecting
///   wholly unrelated names;
/// - two candidates tied at the best distance mean the intent is ambiguous,
///   so nothing is suggested at all.
pub(crate) fn nearest_match<'a, I>(unknown: &str, candidates: I) -> Option<&'a str>
where
    I: IntoIterator<Item = &'a str>,
{
    if unknown.len() < 3 {
        return None;
    }
    let max_distance = unknown.len() / 3;
    let mut best: Option<(usize, &'a str)> = None;
    let mut tied = false;
    for candidate in candidates {
        let distance = edit_distance(unknown, candidate);
        if distance > max_distance {
            continue;
        }
        match best {
            None => best = Some((distance, candidate)),
            Some((best_distance, _)) if distance < best_distance => {
                best = Some((distance, candidate));
                tied = false;
            }
            Some((best_distance, _)) if distance == best_distance => tied = true,
            Some(_) => {}
        }
    }
    let (_, candidate) = best?;
    (!tied).then_some(candidate)
}

#[cfg(test)]
mod tests {
    use super::{edit_distance, nearest_match};

    #[test]
    fn edit_distance_counts_inserts_deletes_substitutions() {
        assert_eq!(edit_distance("", ""), 0);
        assert_eq!(edit_distance("abc", "abc"), 0);
        assert_eq!(edit_distance("abc", ""), 3);
        assert_eq!(edit_distance("", "abc"), 3);
        assert_eq!(edit_distance("tex", "text"), 1);
        assert_eq!(edit_distance("margn", "margin"), 1);
        assert_eq!(edit_distance("kitten", "sitting"), 3);
    }

    #[test]
    fn edit_distance_counts_adjacent_transposition_as_one_edit() {
        assert_eq!(edit_distance("wdith", "width"), 1);
        assert_eq!(edit_distance("hieght", "height"), 1);
    }

    #[test]
    fn edit_distance_is_case_sensitive() {
        assert_eq!(edit_distance("Width", "width"), 1);
    }

    #[test]
    fn close_typo_matches_nearest_candidate() {
        assert_eq!(
            nearest_match("margn", ["paper", "margin", "numbering"]),
            Some("margin")
        );
        assert_eq!(
            nearest_match("tex", ["page", "text", "document", "image"]),
            Some("text")
        );
        assert_eq!(
            nearest_match("wdith", ["src", "path", "alt", "width", "height", "label"]),
            Some("width")
        );
    }

    #[test]
    fn exact_candidate_wins_over_near_misses() {
        assert_eq!(nearest_match("width", ["width", "height"]), Some("width"));
    }

    #[test]
    fn far_off_name_matches_nothing() {
        assert_eq!(
            nearest_match("banana", ["paper", "margin", "numbering"]),
            None
        );
    }

    #[test]
    fn tied_candidates_match_nothing() {
        // `abc` sits one substitution from both `abx` and `aby`; guessing
        // between them would be a coin flip, so no suggestion.
        assert_eq!(nearest_match("abc", ["abx", "aby"]), None);
    }

    #[test]
    fn tie_is_reset_when_a_strictly_closer_candidate_appears() {
        // `abx`/`aby` tie at distance 1, then the exact match at distance 0
        // breaks the tie: the earlier ambiguity no longer applies.
        assert_eq!(nearest_match("abc", ["abx", "aby", "abc"]), Some("abc"));
    }

    #[test]
    fn short_name_matches_nothing() {
        assert_eq!(nearest_match("ab", ["ax"]), None);
    }
}
