//! Unit tests for the degraded-cause self-report: who it counts, who it stays silent about, and the
//! three claims the message is allowed to make.

use super::degraded_files_warning;
use crate::analyze::assemble::DegradedFile;
use crate::pipeline::DegradeCause;
use crate::EngineConfig;

fn file(rel: &str, cause: DegradeCause, dispatched: bool) -> DegradedFile {
    DegradedFile {
        rel: rel.to_string(),
        cause,
        dispatched,
    }
}

fn config() -> EngineConfig {
    EngineConfig {
        size_cap: 12345,
        ..EngineConfig::default()
    }
}

#[test]
fn no_degraded_file_is_silence() {
    assert!(degraded_files_warning(&[], &config()).is_none());
}

#[test]
fn an_undispatched_oversized_file_lost_nothing_and_is_not_counted() {
    // A 4MB `.png`/`.json` degrades, but no parser was ever going to run on it and its line-scan rules
    // still did — counting it would claim a loss that never happened.
    let degraded = [
        file("assets/hero.png", DegradeCause::Oversized, false),
        file("data/dump.json", DegradeCause::Oversized, false),
    ];
    assert!(degraded_files_warning(&degraded, &config()).is_none());
}

#[test]
fn an_undispatched_unreadable_file_is_counted_because_even_its_text_was_lost() {
    let degraded = [file("data/locked.json", DegradeCause::Unreadable, false)];
    let w = degraded_files_warning(&degraded, &config())
        .expect("unreadable is reported at any extension");
    assert!(w.contains("1 file(s)"), "{w}");
    assert!(w.contains("unreadable"), "{w}");
    assert!(w.contains("data/locked.json"), "{w}");
}

#[test]
fn each_cause_gets_its_own_count_sample_and_lever() {
    let degraded = [
        file("big.ts", DegradeCause::Oversized, true),
        file("broken.py", DegradeCause::ParseFailure, true),
        file("gone.go", DegradeCause::Unreadable, true),
    ];
    let w = degraded_files_warning(&degraded, &config()).unwrap();
    assert!(w.contains("3 file(s)"), "{w}");
    assert!(w.contains("1 over the size cap"), "{w}");
    assert!(w.contains("big.ts"), "{w}");
    assert!(
        w.contains("1 dispatched to a native parser that failed to parse"),
        "{w}"
    );
    assert!(w.contains("broken.py"), "{w}");
    assert!(w.contains("1 unreadable"), "{w}");
    assert!(w.contains("gone.go"), "{w}");
    // The three levers.
    assert!(w.contains("`sizeCap`"), "{w}");
    assert!(w.contains("currently 12345 bytes"), "{w}");
    assert!(w.contains("check permissions"), "{w}");
    assert!(w.contains("bug worth reporting"), "{w}");
}

#[test]
fn the_sample_is_capped_per_cause_and_the_overflow_is_disclosed() {
    let mut degraded: Vec<DegradedFile> = (0..7)
        .map(|i| file(&format!("src/big{i}.ts"), DegradeCause::Oversized, true))
        .collect();
    // One parse failure among many oversized files: the cap is per cause precisely so this one keeps a
    // slot instead of being crowded out by the lever the reader already understands.
    degraded.push(file("src/broken.ts", DegradeCause::ParseFailure, true));
    let w = degraded_files_warning(&degraded, &config()).unwrap();
    assert!(w.contains("8 file(s)"), "{w}");
    assert!(
        w.contains("src/big0.ts, src/big1.ts, src/big2.ts, +4 more"),
        "{w}"
    );
    assert!(!w.contains("src/big3.ts"), "{w}");
    assert!(w.contains("src/broken.ts"), "{w}");
}

#[test]
fn the_message_never_claims_the_files_were_skipped() {
    let degraded = [file("big.ts", DegradeCause::Oversized, true)];
    let w = degraded_files_warning(&degraded, &config()).unwrap();
    // The precision the `minified`/`degraded` split exists to protect: line-scan rules DID run.
    assert!(w.contains("`line-scan`"), "{w}");
    assert!(w.contains("SILENT rather than clean"), "{w}");
    assert!(!w.to_lowercase().contains("skipped"), "{w}");
}

#[test]
fn cause_order_is_fixed_rather_than_count_sorted() {
    // Two runs whose cause MIX differs must still read in the same order, so the two can be diffed.
    let a = [
        file("a.ts", DegradeCause::Oversized, true),
        file("b.ts", DegradeCause::ParseFailure, true),
        file("c.ts", DegradeCause::ParseFailure, true),
    ];
    let b = [
        file("a.ts", DegradeCause::Oversized, true),
        file("d.ts", DegradeCause::Oversized, true),
        file("b.ts", DegradeCause::ParseFailure, true),
    ];
    for w in [
        degraded_files_warning(&a, &config()).unwrap(),
        degraded_files_warning(&b, &config()).unwrap(),
    ] {
        let size_at = w.find("over the size cap").unwrap();
        let parse_at = w.find("failed to parse them").unwrap();
        assert!(size_at < parse_at, "{w}");
    }
}
