//! The CAPABILITY × MEASURED cross — one owner for the rule that answers "this build declares it can
//! fill channel C from extension E, the tree is substantially made of E, and E yielded zero C".
//!
//! ## Why it lives here rather than beside either consumer
//!
//! It has two consumers in two lanes and they cannot share a substrate: the coverage reply
//! (`zzop coverage` → `trees[].ioChannels.zeroExtraction`) computes it from an ALREADY-ASSEMBLED tree
//! JSON, which may have been produced by a different run entirely, while the analyze warning computes
//! it mid-assembly from typed io facts. Neither can call the other. What they must not do is each own
//! a copy of the RULE — the recognizer cross, the db-kind partition, and the principal-share floor are
//! exactly the parts that would drift, and this repo has measured that drift three times. So the rule
//! lives here as a pure function over two measurements, and each lane supplies its own.
//!
//! ## What the floor is for
//!
//! Without it the cross is arithmetically true and practically unreadable. The constant is
//! [`crate::MIN_UNCOVERED_EXTENSION_SHARE_PCT`], the same one the `NO loaded DSL rule targets …`
//! reports and the `unreadExtensions` cell use, because all of them ask "is this a filetype the tree is
//! MADE OF" and that question has one answer. The measurement that settled the floor — and the
//! denominator argument that distinguishes this cross from `unreadExtensions`' — is not restated here:
//! `zzop_facade`'s `query_coverage::io_channels::zero_extraction` doc owns it, and a second copy would
//! be the drift this module exists to prevent.

use std::collections::{BTreeMap, BTreeSet};

use zzop_core::recognizer::channel;

/// One `(channel, extension)` pair this build can fill and this tree did not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZeroExtractionRow {
    /// The MEASURED channel this row counts — one of [`io_side_channels`]'s three.
    pub channel: &'static str,
    /// Lowercased extension, no dot.
    pub ext: String,
    /// How many structurally-read files of that extension the tree has.
    pub structural_files: usize,
    /// Every framework this build declares a recognizer for on that channel/extension, sorted.
    ///
    /// The join is per-RECOGNIZER and both halves at once: one row of the capability table must name
    /// this channel in its `emits` AND this extension in its `extensions`. It read as two independent
    /// filters until 2026-08-26 — not in the code, which always ANDed them, but in the DECLARATIONS,
    /// where one `io.provides:db-table` spelling covered both sides of the db kind. The measurement
    /// here counts the provide side only, so three consume-only recognizers (`prisma client`,
    /// typescript's and rust's `raw sql`) were listed as capability behind a provide-side zero. On
    /// immich that put `raw sql` beside a 0 taken over 24 `.ts` files holding 77 `CREATE TABLE`s, and
    /// the honest reading of the row ("this build has a `.ts` raw-SQL table recognizer and it found
    /// nothing") is the opposite of the truth ("this build does not read `CREATE TABLE` inside
    /// `.ts`"). The spelling now splits by side (`channel::DB_PROVIDES` / `channel::DB_CONSUMES`) and
    /// the contract that binds `emits` to each recognizer's own code consults the side.
    pub recognizers: Vec<&'static str>,
}

/// The io kind [`channel::DB_PROVIDES`] carves out, read off that constant's own suffix rather than spelled a
/// second time: `channel::DB_PROVIDES` IS `"io.provides:db-table"`, so the two can never drift.
pub fn db_kind() -> &'static str {
    channel::DB_PROVIDES.rsplit(':').next().unwrap_or_default()
}

/// The recognizer channels that correspond to an io SIDE, and how each partitions the io records.
/// `channel::AUTH_EVIDENCE` is deliberately absent — it is not an io side at all (its own doc), so a
/// tree cannot contribute to it and no count of it would mean anything.
///
/// Returned as `(channel, reads_provides, is_db_kind)`. The db partition is what keeps `io.provides`
/// honest on a tree like gogs: its 12 `db-table` provides belong to [`channel::DB_PROVIDES`]'s row and must not
/// be allowed to fill the route-provide row.
///
/// [`channel::DB_CONSUMES`] is declarable but NOT measured here, and the asymmetry is deliberate
/// rather than an oversight: this table is the row POPULATION of the coverage cross, and widening it
/// is the open judgment `2.backlog/undecided.md` U117 owns. What the split bought here is the other
/// half — the three rows that DO ship can no longer name a recognizer that fills the opposite side.
pub fn io_side_channels() -> [(&'static str, bool, bool); 3] {
    [
        (channel::PROVIDES, true, false),
        (channel::CONSUMES, false, false),
        (channel::DB_PROVIDES, true, true),
    ]
}

/// The STRUCTURAL half of the cross, measured from typed facts — extension → count of files a parser
/// frontend projected at least one fact from.
///
/// The definition lives here because the cross above is what consumes it, and because the coverage
/// reply measures the same thing off an assembled tree instead: a file is structural when it appears in
/// the symbol channel, the dep channel, or the io channel, and is not degraded. The io channel is the
/// one that is easy to forget and the one that cost a measurement — `zzop-parser-sql` and Prisma
/// project neither symbols nor dep edges by design, so a tree's only `db-table` source read as
/// unparsed. A caller that omits a channel here understates the denominator and silently narrows every
/// row the cross can produce.
pub fn structural_by_ext<'a>(
    all_rels: impl Iterator<Item = &'a str>,
    structural: &std::collections::HashSet<&str>,
    degraded: &std::collections::HashSet<&str>,
) -> BTreeMap<String, usize> {
    let mut out: BTreeMap<String, usize> = BTreeMap::new();
    for rel in all_rels {
        if degraded.contains(rel) || !structural.contains(rel) {
            continue;
        }
        *out.entry(ext_of(rel)).or_default() += 1;
    }
    out
}

/// Lowercased extension of a rel path, falling back to the whole basename when there is no dot — the
/// same keying the coverage reply's `extensions` rows use, so a row here and a row there name the same
/// thing.
pub fn ext_of(rel: &str) -> String {
    let base = rel.rsplit('/').next().unwrap_or(rel);
    match base.rsplit_once('.') {
        Some((_, ext)) if !ext.is_empty() => ext.to_ascii_lowercase(),
        _ => base.to_ascii_lowercase(),
    }
}

/// The MEASURED half, from typed io facts — channel → extension → count.
///
/// The db partition is what keeps `io.provides` honest: a tree's `db-table` provides belong to
/// [`channel::DB_PROVIDES`]'s row, and letting them fill the route-provide row is exactly how gogs read as
/// "contributed joinable io" while ~300 routes were invisible. The coverage reply counts the same
/// three partitions off an assembled tree's arrays instead — same partitions, different substrate.
pub fn extracted_by_channel(
    io_provides: &[zzop_core::IoProvide],
    io_consumes: &[zzop_core::IoConsume],
) -> BTreeMap<&'static str, BTreeMap<String, usize>> {
    io_side_channels()
        .into_iter()
        .map(|(chan, reads_provides, is_db)| {
            let mut counts: BTreeMap<String, usize> = BTreeMap::new();
            if reads_provides {
                for p in io_provides
                    .iter()
                    .filter(|p| (p.kind == db_kind()) == is_db)
                {
                    *counts.entry(ext_of(&p.file)).or_default() += 1;
                }
            } else {
                for c in io_consumes
                    .iter()
                    .filter(|c| (c.kind == db_kind()) == is_db)
                {
                    *counts.entry(ext_of(&c.file)).or_default() += 1;
                }
            }
            (chan, counts)
        })
        .collect()
}

/// The cross itself.
///
/// `structural_files` is extension → count of files this tree READ structurally. `extracted` is, per
/// channel, extension → count of io records that channel's partition produced; a channel absent from
/// the map, or an extension absent from its inner map, counts as zero, which is the whole subject.
///
/// Determinism: both inputs are `BTreeMap`s and the recognizer set is a `BTreeSet`, so the result is
/// sorted on `(channel, ext)` rather than on the tree's file order.
pub fn zero_extraction_rows(
    structural_files: &BTreeMap<String, usize>,
    extracted: &BTreeMap<&'static str, BTreeMap<String, usize>>,
) -> Vec<ZeroExtractionRow> {
    let recognizers = crate::framework_recognizers();
    let structural_total: usize = structural_files.values().sum();
    let mut rows: BTreeMap<(&'static str, &str), BTreeSet<&'static str>> = BTreeMap::new();
    for (chan, _, _) in io_side_channels() {
        let counts = extracted.get(chan);
        for (ext, files) in structural_files {
            if *files == 0 || counts.and_then(|c| c.get(ext)).copied().unwrap_or(0) > 0 {
                continue;
            }
            // Integer-floored share, the same arithmetic `unreadExtensions` and the engine's reports use.
            if structural_total == 0
                || files * 100 / structural_total < crate::MIN_UNCOVERED_EXTENSION_SHARE_PCT
            {
                continue;
            }
            // ONE recognizer row must carry both halves. An empty result is therefore not a weaker
            // row that gets printed anyway — it is the statement "no recognizer in this build fills
            // this channel from this extension", and under this population it can only mean the
            // extension is not a language this build reads for that channel at all, which
            // `frameworkRecognizers` and `unreadExtensions` already answer without a per-tree row.
            // Printing it here would add rows like "no recognizer extracts http routes from .sql",
            // arithmetically true and exactly the unreadable class the principal-share floor exists
            // to keep out.
            let capable: BTreeSet<&'static str> = recognizers
                .iter()
                .filter(|r| r.emits.contains(&chan) && r.extensions.contains(&ext.as_str()))
                .map(|r| r.framework)
                .collect();
            if !capable.is_empty() {
                rows.insert((chan, ext.as_str()), capable);
            }
        }
    }
    rows.into_iter()
        .map(|((channel, ext), names)| ZeroExtractionRow {
            channel,
            ext: ext.to_string(),
            structural_files: structural_files.get(ext).copied().unwrap_or(0),
            recognizers: names.into_iter().collect(),
        })
        .collect()
}
