//! S11 — the unread-io-kind self-report.
//!
//! `zzop_core::IoKind` is an open `String`, and that openness is advertised as an extension point: a
//! Mode B adapter may emit any kind it likes. What the advertisement omits is that emitting one no RULE
//! reads produces facts with no findings — every rule compares against a kind literal, and
//! `zzop_core::RULE_READ_IO_KINDS` is the set those literals form.
//!
//! ⚠ Edges are NOT part of that claim, and the first cut of this module said they were. The cross-layer
//! linker is kind-agnostic (it joins on the composite `(kind, key)` and compares the kind to nothing), so
//! an unread kind still joins and still yields edges. Saying otherwise made this tripwire contradict the
//! `buckets`/`edges` fields sitting in the same reply — a self-report that can be falsified by the
//! document it rides in is worse than none.
//!
//! So the failure mode is the repo's most expensive one wearing a new hat: an author injects `"queue"`
//! facts, the coverage census (kind-agnostic by design) confirms the channel filled, the reply carries
//! zero findings, and every signal available says the analysis ran clean. This tripwire is the sentence
//! that separates "nothing to report" from "nobody read it".
//!
//! Sibling of S1-S10 in shape but not in trigger: those infer silence from a SHAPE the tree has (a
//! controller-looking file, a server-framework import). This one needs no inference at all — the fact is
//! present and its kind is right there, so there is no false-positive direction to trade against.

use std::collections::BTreeMap;

use zzop_core::{IoConsume, IoProvide, RULE_READ_IO_KINDS};

/// Names in the message, at most this many per kind — enough to go look, bounded so one adapter's
/// thousand rows cannot crowd out the run's other warnings.
const MAX_EXAMPLES: usize = 3;

/// One warning naming every io kind present in this tree that NO RULE reads, with counts and example
/// files. `None` when every kind present is read (the overwhelmingly common case, including every tree
/// with no io facts at all). Says nothing about the linker, which reads no kind literal at all.
pub fn unread_io_kind_warning(provides: &[IoProvide], consumes: &[IoConsume]) -> Option<String> {
    // (count, example files) per unread kind. BTreeMap so the message is deterministic without a sort.
    // Owned strings rather than borrows: the map outlives the closure's borrow of each fact, and the
    // allocation only ever happens on the unread path, which is empty for nearly every tree.
    let mut unread: BTreeMap<String, (usize, Vec<String>)> = BTreeMap::new();
    let mut note = |kind: &str, file: &str| {
        if RULE_READ_IO_KINDS.contains(&kind) {
            return;
        }
        let entry = unread.entry(kind.to_string()).or_insert((0, Vec::new()));
        entry.0 += 1;
        if entry.1.len() < MAX_EXAMPLES && !entry.1.iter().any(|f| f == file) {
            entry.1.push(file.to_string());
        }
    };
    for p in provides {
        note(&p.kind, &p.file);
    }
    for c in consumes {
        note(&c.kind, &c.file);
    }
    if unread.is_empty() {
        return None;
    }

    let per_kind: Vec<String> = unread
        .iter()
        .map(|(kind, (count, examples))| {
            format!("\"{kind}\" ({count} fact(s), e.g. {})", examples.join(", "))
        })
        .collect();
    let read = RULE_READ_IO_KINDS
        .iter()
        .map(|k| format!("\"{k}\""))
        .collect::<Vec<_>>()
        .join("/");
    Some(format!(
        "Unread io kind(s): this tree carried {} whose kind NO RULE in this build reads — this build's \
         rules read {read}. The facts were extracted, are counted in the coverage census, and WILL \
         still join across trees: the cross-layer linker matches on (kind, key) without comparing the \
         kind to anything, so a provide and a consume of the same unread kind still produce an edge. \
         What they cannot produce is a FINDING — so zero findings about them means NOT ANALYZED, never \
         \"nothing wrong\". IoKind is deliberately open so an adapter can carry facts a future build \
         will read; until a rule exists, this line is the only thing that distinguishes carrying them \
         from acting on them.",
        per_kind.join(", ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provide(kind: &str, file: &str) -> IoProvide {
        IoProvide {
            response: None,
            kind: kind.to_string(),
            key: "k".to_string(),
            file: file.to_string(),
            line: 1,
            symbol: None,
            body: None,
        }
    }

    #[test]
    fn every_read_kind_is_silent() {
        let ps: Vec<IoProvide> = RULE_READ_IO_KINDS
            .iter()
            .map(|k| provide(k, "a.ts"))
            .collect();
        assert!(unread_io_kind_warning(&ps, &[]).is_none());
        assert!(unread_io_kind_warning(&[], &[]).is_none());
    }

    #[test]
    fn an_unread_kind_is_named_with_its_count_and_a_file() {
        let ps = vec![
            provide("queue", "worker/a.ts"),
            provide("queue", "worker/b.ts"),
            provide("http", "api/x.ts"),
        ];
        let w = unread_io_kind_warning(&ps, &[]).expect("warning");
        assert!(w.contains("\"queue\" (2 fact(s)"), "{w}");
        assert!(w.contains("worker/a.ts"), "{w}");
        assert!(
            !w.contains("\"http\" ("),
            "a read kind must not be reported: {w}"
        );
        // The two readings this line exists to separate must both be spelled out.
        assert!(w.contains("NOT ANALYZED"), "{w}");
    }

    /// The first cut of this message said an unread kind produces "no finding and no cross-layer edge".
    /// The second half was false — `link_cross_layer_io` joins on the composite `(kind, key)` and
    /// compares the kind to nothing, so an unread kind DOES join. A warning that a reader can falsify
    /// against the `edges` array in the same reply teaches them to distrust the whole channel.
    #[test]
    fn the_message_does_not_deny_edges_it_cannot_prevent() {
        let w = unread_io_kind_warning(&[provide("queue", "worker/a.ts")], &[]).expect("warning");
        assert!(
            !w.contains("no cross-layer edge"),
            "the linker is kind-agnostic — this claim is false: {w}"
        );
        assert!(
            w.contains("still join"),
            "the message must say the facts DO still join: {w}"
        );
        assert!(
            w.contains("cannot produce is a FINDING"),
            "and must name what is actually lost: {w}"
        );
    }

    /// The compose-phase sentinels are NOT in `RULE_READ_IO_KINDS`, so a surviving one is reported — which
    /// is correct and deliberate: compose strips every sentinel, so one reaching assembly is a bug, and
    /// this report is how it becomes visible rather than being silently listed as a read kind.
    #[test]
    fn a_surviving_compose_sentinel_is_reported_rather_than_whitelisted() {
        let w = unread_io_kind_warning(&[provide("nest-global-prefix", "a.ts")], &[])
            .expect("a sentinel that survived compose must not be silent");
        assert!(w.contains("nest-global-prefix"), "{w}");
    }
}
