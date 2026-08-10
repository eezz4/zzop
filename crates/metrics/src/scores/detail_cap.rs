//! The ONE place the score reports' detail-list caps live, and the ONE way a capped list says what it
//! dropped.
//!
//! ## Why this module exists (2026-07-31)
//! Ten `scores/*` modules each declared their own `MAX_DETAIL_ITEMS`/`MAX_VIOLATIONS_LISTED` and then
//! called a bare `Vec::truncate`. That is a SILENT cap: `godFile.files` came back with 50 rows and
//! nothing anywhere said whether the 51st existed, so "there are exactly 50 god files" and "there are
//! four hundred and you are seeing an eighth of them" shipped as identical bytes.
//!
//! Every other shipped list in this repo already discloses: `findings` carries `{shown, truncated}`,
//! `suggestionsTruncated`/`edgesTruncated`/`degradedTruncated` count what a cap left out, and the graph
//! lane prints a `%%` drawn/inScope/total census. The score lane was the exception, so it is the one that
//! moved.
//!
//! ## The convention, chosen once
//! Beside each capped list sits a `<listField>Truncated` count of the rows the cap dropped, matching the
//! `<list>Truncated` spelling the summary layer already ships. A COUNT rather than a bool, because the
//! honest number is available for free at the truncation site and `list.len() + <list>Truncated` then
//! reconstructs the full total — which several of these reports otherwise never publish at all
//! (`godFile`, `diamond`, `lod`, `typeSafety`, `hierarchy`, `publicApi`, `siblingCross` all compute their
//! score from a count they do not carry). `busFactor.risky` and `rename.renamed` do carry theirs, and the
//! sibling still rides on those two: one convention across the lane beats a per-module judgment about
//! whether a reader can already derive it.
//!
//! ALWAYS SERIALIZED, including as `0`, unlike the summary layer's omit-when-absent truncation fields.
//! These are fixed-shape report structs whose every other scalar is unconditional, and the failure being
//! repaired here is silence: a field that vanishes when nothing was dropped makes "complete list" and
//! "this build has no disclosure" the same bytes again, one level down.
//!
//! ## Two caps, and the axis that separates them
//! [`MAX_FILE_ROWS_LISTED`] governs the per-FILE and per-PAIR lists; [`MAX_EDGE_ROWS_LISTED`] is double
//! it and governs the per-EDGE lists (`hierarchy`, `publicApi`, `siblingCross`), whose rows are
//! `from -> to` pairs and therefore far more numerous over the same tree — one file can contribute
//! dozens. Both values are the ones those modules already shipped; centralizing them here changes what a
//! reader is TOLD, not what they are shown.

/// Cap for per-FILE and per-PAIR detail lists (`file_size_compliance`, `godFile`, `diamond`, `rename`, `busFactor`,
/// `typeSafety`, `lod`). One row per file (or per root/leaf pair), so a tree's row count is bounded by
/// its file count.
pub(crate) const MAX_FILE_ROWS_LISTED: usize = 50;

/// Cap for per-EDGE detail lists (`hierarchy`, `publicApi`, `siblingCross`). One row per import edge —
/// a single file contributes as many rows as it has imports — so this list runs longer than a per-file
/// one over the same tree, which is why it is double [`MAX_FILE_ROWS_LISTED`].
pub(crate) const MAX_EDGE_ROWS_LISTED: usize = 100;

/// Truncates `items` to `cap` and returns HOW MANY ROWS IT DROPPED — the number that ships beside the
/// list as `<listField>Truncated`. `0` means the list is complete.
///
/// The truncation and its disclosure are one call on purpose: the dropped count is only knowable before
/// the `truncate`, and every bug this module repairs was a `truncate` that happened somewhere the count
/// was no longer available to state.
pub(crate) fn cap_and_count_dropped<T>(items: &mut Vec<T>, cap: usize) -> u32 {
    let dropped = items.len().saturating_sub(cap);
    items.truncate(cap);
    // `usize -> u32` cannot overflow in practice (a row per file/edge), and saturating rather than
    // wrapping keeps an absurd input from reporting a SMALL remainder, which would be the same lie in a
    // new costume.
    u32::try_from(dropped).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_short_list_is_untouched_and_reports_zero_dropped() {
        let mut items: Vec<u8> = (0..3).collect();
        assert_eq!(cap_and_count_dropped(&mut items, 50), 0);
        assert_eq!(items.len(), 3);
    }

    /// The boundary: exactly `cap` rows is a COMPLETE list, not a truncated one. Off by one here and
    /// every full-but-not-over list would claim a phantom remainder.
    #[test]
    fn exactly_at_the_cap_reports_zero_dropped() {
        let mut items: Vec<u8> = (0..50).collect();
        assert_eq!(cap_and_count_dropped(&mut items, 50), 0);
        assert_eq!(items.len(), 50);
    }

    #[test]
    fn over_the_cap_truncates_and_reports_the_remainder() {
        let mut items: Vec<u16> = (0..137).collect();
        assert_eq!(cap_and_count_dropped(&mut items, 50), 87);
        assert_eq!(items.len(), 50);
        // The kept rows are the FIRST ones: this helper never reorders, so which rows survive is
        // decided entirely by the caller's own sort. That is NOT uniformly "the worst ones". The seven
        // `MAX_FILE_ROWS_LISTED` callers sort by severity/size first (`Reverse(...)` or density
        // descending), so for them first == worst. The three `MAX_EDGE_ROWS_LISTED` callers
        // (`hierarchy`, `public_api`, `sibling_cross`) sort ALPHABETICALLY BY MODULE, so their kept
        // rows are the alphabetically-first modules and the dropped ones are not the milder ones —
        // which is exactly why the `*Truncated` sibling ships beside every list rather than only
        // beside the ranked ones: the count is the only honest thing to say about a cut a reader
        // cannot rank for themselves.
        assert_eq!(items[0], 0);
        assert_eq!(items[49], 49);
    }
}
