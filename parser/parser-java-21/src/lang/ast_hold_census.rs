//! The counterfactual R1 has never had a number for: what does it cost to NOT drop the AST?
//!
//! `architecture.md`'s constraint ledger records R1 (tree streaming + AST drop) as the root of nearly
//! every other limitation this project accepts, and its witness table carries exactly one measurement —
//! **169.5 MB peak working set** on the corpus's largest tree, cold. That number describes the design
//! that WON. The design it beat has never been measured, and the ledger says so in its own words:
//! it records that nobody ever measured what holding the AST per tree would have cost
//! (review ledger V148).
//!
//! So R1 stands on an argument rather than on a pair of numbers, and it fixes the answer to four other
//! questions (the missing type tier, the declaration-based workaround, the 21.6% import resolution).
//! An unmeasured root is worth measuring even when — especially when — you expect it to hold.
//!
//! ## What this measures, and what it deliberately does not
//!
//! ONE frontend, on the population the witness table used. `corpus/oss/the-algorithm` is 1,043 `.java`
//! files of the 1,276 the engine structurally parses there, so this covers ~82% of that tree's parsed
//! population with the real parser, on the real files.
//!
//! It is NOT a whole-engine counterfactual: swc and ruff hold their ASTs differently, and a Java figure
//! does not become a TypeScript figure. What it produces is the first REAL number for the shape of the
//! cost — and the shape is what R1's argument is about.
//!
//! ## How to run it — the A/B, and why it is an A/B
//!
//! ```text
//! ZZOP_HOLD_ROOT=<abs path to a tree>  cargo test -p zzop-parser-java-21 --lib ast_hold -- --ignored --nocapture
//! ZZOP_HOLD_ROOT=<abs path> ZZOP_HOLD_ASTS=1  (same command)
//! ```
//! The two arms walk the SAME files with the SAME parser and differ in one statement: whether the parsed
//! tree is pushed into a `Vec` or dropped at the end of the loop. Peak memory is read from OUTSIDE, by
//! the method `architecture.md` already documents for the 169.5 MB figure — `Start-Process` plus
//! `PeakWorkingSet64` polling, stdout redirected to a file. ⚠ That redirect is load-bearing: piping the
//! output back makes the poller race the exit.
//!
//! Alternating, not batched: run A, B, A, B in one window. The cheat sheet's §5.40 rule about wall-clock
//! A/B applies to memory for the same reason — a machine that gets busier between two batched runs
//! attributes its own weather to the change under test.

#[cfg(test)]
mod tests {
    /// Every `.java` file under `dir`, in a deterministic order so the two arms read the same bytes in
    /// the same sequence. Order matters here: an allocator's peak depends on the sequence of requests.
    fn java_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        let mut names: Vec<std::path::PathBuf> = entries.flatten().map(|e| e.path()).collect();
        names.sort();
        for p in names {
            if p.is_dir() {
                if p.file_name().and_then(|n| n.to_str()) != Some(".git") {
                    java_files(&p, out);
                }
            } else if p.extension().and_then(|e| e.to_str()) == Some("java") {
                out.push(p);
            }
        }
    }

    /// See the module doc. Ignored because it walks a checkout and answers a question about a design
    /// decision, not about this crate's behaviour.
    // Item-level, not file-level: `policy_value_pins` refuses a `#![allow]` that uncovers a whole
    // module, and it is right to — this exemption covers ONE function, whose printed line IS its
    // product. A census whose result nobody can read has measured nothing.
    #[allow(clippy::print_stdout)]
    #[test]
    #[ignore = "walks a corpus and is half of an A/B; see this module's doc for how to run both arms"]
    fn ast_hold_vs_drop_peak() {
        let Ok(root) = std::env::var("ZZOP_HOLD_ROOT") else {
            panic!("set ZZOP_HOLD_ROOT to a tree to walk -- an unset root would measure nothing and say so as a pass");
        };
        let root = std::path::PathBuf::from(root);
        assert!(
            root.is_dir(),
            "ZZOP_HOLD_ROOT {} is not a directory",
            root.display()
        );

        let mut files = Vec::new();
        java_files(&root, &mut files);
        assert!(
            !files.is_empty(),
            "no .java file under {} -- an empty population is not a measurement of zero",
            root.display()
        );

        let hold = std::env::var("ZZOP_HOLD_ASTS").is_ok_and(|v| v == "1");
        // The one statement the two arms differ in. Both parse every file with the shipped parser.
        let mut held: Vec<(String, tree_sitter::Tree)> = Vec::new();
        let mut parsed = 0usize;
        let mut bytes = 0usize;
        for f in &files {
            let Ok(text) = std::fs::read_to_string(f) else {
                continue;
            };
            bytes += text.len();
            let Some(tree) = crate::parse_tree(&text) else {
                continue;
            };
            parsed += 1;
            if hold {
                held.push((text, tree));
            }
        }

        println!(
            "arm={}  files={}  parsed={}  source_bytes={}  retained_trees={}",
            if hold { "HOLD" } else { "DROP" },
            files.len(),
            parsed,
            bytes,
            held.len()
        );
        // Touch the vector after printing so nothing above can be optimised into a drop.
        assert_eq!(held.len(), if hold { parsed } else { 0 });
    }
}
