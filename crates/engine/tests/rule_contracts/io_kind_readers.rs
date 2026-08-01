//! Binds `zzop_core::RULE_READ_IO_KINDS` to the io-kind literals this workspace actually compares.
//!
//! That constant is what the S11 unread-io-kind self-report subtracts against, so it decides which
//! kinds a run calls UNREAD. Both drift directions are silent:
//!
//!   a kind wired but unlisted  -> S11 reports a kind that IS read; the report cries wolf and the next
//!                                 reader learns to ignore it
//!   a kind listed but unwired  -> S11 stays quiet about a kind nothing reads, which is the exact
//!                                 silence the report exists to break
//!
//! The subject set is the `kind == "..."` comparisons in non-test source. Test files are excluded
//! deliberately: a fixture asserts on whatever kind it invented, and letting fixtures widen the list
//! would let a test silence a production report.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Kinds that are compared in production source but are NOT joined — each needs a reason, because an
/// unexplained entry here is how a real unwired kind hides.
const COMPARED_BUT_NOT_JOINED: &[(&str, &str)] = &[
    (
        "nest-global-prefix",
        "compose-phase sentinel: `analyze::compose::global_prefix` compares it in order to STRIP it, so \
         it never survives to assembly. Listing it as joined would silence the report that catches one \
         that did survive.",
    ),
    (
        "client-base-prefix",
        "compose-phase sentinel, same shape as nest-global-prefix — `analyze::compose::client_base` \
         compares it to strip it.",
    ),
];

/// Every `kind == "..."` literal in non-test Rust source under the directories that hold rules and
/// engine logic.
fn compared_kind_literals() -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for dir in ["crates", "rules", "parser"] {
        collect(&repo().join(dir), &mut out);
    }
    out
}

fn collect(dir: &Path, out: &mut BTreeSet<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let path = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        if path.is_dir() {
            if name != "target" && name != "tests" {
                collect(&path, out);
            }
            continue;
        }
        // `tests.rs` / `*_tests.rs` are this workspace's in-crate test convention; skip both.
        if !name.ends_with(".rs") || name == "tests.rs" || name.ends_with("_tests.rs") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        for (i, _) in text.match_indices("kind == \"") {
            let rest = &text[i + "kind == \"".len()..];
            if let Some(end) = rest.find('"') {
                let kind = &rest[..end];
                // A kind is kebab-case lowercase by convention; anything else is a different construct
                // that happens to spell `kind == "`, and guessing about it would be worse than skipping.
                if !kind.is_empty() && kind.chars().all(|c| c.is_ascii_lowercase() || c == '-') {
                    out.insert(kind.to_string());
                }
            }
        }
    }
}

#[test]
fn every_compared_io_kind_is_joined_or_explicitly_excused() {
    let compared = compared_kind_literals();
    assert!(
        !compared.is_empty(),
        "found zero `kind == \"...\"` comparisons — the scan root is wrong, and an empty subject set \
         would make this test vacuously green"
    );
    let excused: BTreeSet<&str> = COMPARED_BUT_NOT_JOINED.iter().map(|(k, _)| *k).collect();
    let stray: Vec<&String> = compared
        .iter()
        .filter(|k| !zzop_core::RULE_READ_IO_KINDS.contains(&k.as_str()))
        .filter(|k| !excused.contains(k.as_str()))
        .collect();
    assert!(
        stray.is_empty(),
        "io kind(s) compared in production source but neither in RULE_READ_IO_KINDS nor excused: \
         {stray:?}\n\
         If a reader was wired for it, add it to zzop_core::RULE_READ_IO_KINDS so the S11 unread-io-kind \
         report stops calling it unread. If it is a sentinel or some other non-join comparison, add it \
         to COMPARED_BUT_NOT_JOINED with a reason."
    );
}

#[test]
fn every_joined_kind_is_actually_compared_somewhere() {
    let compared = compared_kind_literals();
    let unwired: Vec<&&str> = zzop_core::RULE_READ_IO_KINDS
        .iter()
        .filter(|k| !compared.contains(**k))
        .collect();
    assert!(
        unwired.is_empty(),
        "RULE_READ_IO_KINDS names kind(s) no production comparison reads: {unwired:?} — S11 would stay \
         silent about facts nothing acts on, which is the silence it exists to break"
    );
}

#[test]
fn every_exclusion_carries_a_reason() {
    for (kind, why) in COMPARED_BUT_NOT_JOINED {
        assert!(
            why.len() > 20,
            "{kind}'s exclusion reason is too thin to be a judgment: {why:?}"
        );
    }
}
