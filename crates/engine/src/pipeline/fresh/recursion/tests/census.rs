//! The statement-token census, split out of `tests.rs` on 2026-09-09 for the line cap.
//!
//! It is one test and it is not really a unit test: it walks a checkout. It lives here so the
//! constants it derives can point at a command rather than at a memory of how they were derived.
//!
//! ## What the 2026-09-09 run changed, and what it did not
//!
//! 🔴 `LONGEST_REAL_PYTHON_STATEMENT_MEASURED` was 1_417 and it was not a statement length. The
//! scanner ended a statement only at `;`/`{`/`}`, which Python does not use that way, so it measured
//! the longest comma-free stretch — and in a comma-free file, the whole file. With the boundary
//! corrected (review ledger V140) the census gives **1,328**.
//!
//! 🟢 The same run reproduced the other two constants EXACTLY — `.rs` 626 and `.cjs` 12,734 (the
//! grafana yarn release). That is the check that matters: the boundary moved for one language and
//! for no other, and if this run had disturbed the C-family scan those two numbers would have moved
//! with it.
//!
//! 📏 It also prints extensions with no cap at all (`.md` 12,200, `.astro` 2,575) — those are read by
//! a pre-scan host or by nothing, and they are printed rather than filtered so a future cap can be
//! set from a number instead of from a guess.

// This test PRINTS, and that is its product rather than a debugging leftover: it is run by hand with
// `--ignored --nocapture` to re-derive constants a human then commits. A census that returns a value
// nobody can read has not measured anything.
#![allow(clippy::print_stdout)]

use super::super::bounds::*;
use super::super::*;

/// The census the four `LONGEST_REAL_*_MEASURED` constants come from — as a COMMAND, not a memory.
///
/// 🔴 Those numbers have been wrong twice, and both times the scanner that produced them was the thing
/// that was wrong, not the arithmetic. V129: the identifier arm sat in the regex scanner's inner
/// `match`, so it counted identifier BYTES and the real-code figure read 3-6x too large. V140: the
/// boundary was `;`/`{`/`}` for every language, so a `.py` file was one statement end to end and the
/// Python figure was not a statement length at all.
///
/// ⚠ Both were re-derived by hand, by someone who had to remember how. That is the gap this closes:
/// the census now runs the SHIPPED needle — `statement_policy` picks the cap and the boundary exactly
/// as the gate does — so a scanner change and a re-census cannot disagree about what was measured.
///
/// ```text
/// cargo test -p zzop-engine --lib the_longest_real_statement_census -- --ignored --nocapture
/// ```
/// `ZZOP_CENSUS_ROOT` overrides the corpus root (default `corpus`). Ignored by default because it
/// walks thousands of files and its subject is a checkout, not this crate.
#[test]
#[ignore = "walks a corpus; run it explicitly when re-deriving the bounds -- see this test doc"]
fn the_longest_real_statement_census() {
    use std::collections::BTreeMap;

    fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                if !matches!(
                    p.file_name().and_then(|n| n.to_str()),
                    Some("node_modules" | ".git")
                ) {
                    walk(&p, out);
                }
            } else {
                out.push(p);
            }
        }
    }

    // `cargo test` runs with the CRATE as cwd, not the workspace root, so the default is resolved
    // against the manifest rather than against wherever the shell happened to be.
    let root = std::env::var("ZZOP_CENSUS_ROOT")
        .unwrap_or_else(|_| format!("{}/../../corpus", env!("CARGO_MANIFEST_DIR")));
    let root = std::path::Path::new(&root);
    assert!(
        root.is_dir(),
        "census root {} is not a directory -- a census with no subjects has no answer, not zero",
        root.display()
    );
    let mut files = Vec::new();
    walk(root, &mut files);
    assert!(
        !files.is_empty(),
        "census root {} held no files",
        root.display()
    );

    // ext -> (longest statement seen, the file it was seen in, how many files of that ext were read)
    let mut worst: BTreeMap<String, (usize, String, usize)> = BTreeMap::new();
    for f in &files {
        let Some(ext) = f.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        let rel = f.to_string_lossy().replace('\\', "/");
        let (cap, ends) = statement_policy(&rel);
        if cap.is_none() {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(f) else {
            continue;
        };
        let d = scan_depths(&text, ends);
        let slot = worst
            .entry(ext.to_string())
            .or_insert((0, String::new(), 0));
        slot.2 += 1;
        if d.statement_tokens > slot.0 {
            *slot = (d.statement_tokens, rel, slot.2);
        }
    }

    println!(
        "statement-token census over {} ({} files walked)",
        root.display(),
        files.len()
    );
    for (ext, (max, where_, seen)) in &worst {
        println!("  .{ext:<6} {max:>8} tokens   ({seen} file(s))   {where_}");
    }
    println!(
        "committed bounds: rs {} / ts {} / py {}",
        LONGEST_REAL_RUST_STATEMENT_MEASURED,
        LONGEST_REAL_TYPESCRIPT_STATEMENT_MEASURED,
        LONGEST_REAL_PYTHON_STATEMENT_MEASURED
    );

    // The census is allowed to find a LONGER statement than the committed bound -- that is what it is
    // for. It is not allowed to find one past the CAP, because that is a real file this gate refuses.
    for (ext, (max, where_, _)) in &worst {
        let (cap, _) = statement_policy(&format!("x.{ext}"));
        let Some(cap) = cap else { continue };
        assert!(
            *max <= cap,
            ".{ext}: {where_} holds a {max}-token statement and the cap is {cap} -- this gate refuses \
             a real file. Raise the cap and re-derive its LONGEST_REAL_* twin, or fix the scanner."
        );
    }
}
