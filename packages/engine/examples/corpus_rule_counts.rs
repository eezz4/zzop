//! Ad hoc harness to check finding counts for a specific set of rule ids against a real tree — not
//! a general-purpose tool.
//!
//! Usage: `cargo run --release -p zzop-engine --example corpus_rule_counts -- <root> <rule_id> [<rule_id> ...]`
//! Loads `rules/dsl` from the repo root (relative to CWD), runs `analyze_tree` once, and prints the
//! total finding count plus up to 5 sampled `(file:line)` snippets per requested rule id.

use std::path::PathBuf;

use zzop_engine::{analyze_tree, EngineConfig};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: corpus_rule_counts <root> <rule_id> [<rule_id> ...]");
        std::process::exit(2);
    }
    let root = PathBuf::from(&args[0]);
    let rule_ids = &args[1..];

    let loaded = zzop_core::load_dsl_packs(std::path::Path::new("rules/dsl"));
    eprintln!(
        "packs: {} loaded, {} errors",
        loaded.packs.len(),
        loaded.errors.len()
    );
    for err in &loaded.errors {
        eprintln!("  pack error: {err:?}");
    }
    let config = EngineConfig {
        source_id: "corpus-recheck".to_string(),
        packs: loaded.packs.into_iter().map(|(_, p)| p).collect(),
        ..EngineConfig::default()
    };

    let out = analyze_tree(&root, &config);
    println!(
        "files={} findings_total={}",
        out.file_count,
        out.findings.len()
    );

    // Print warnings verbatim (e.g. minified-file skip, BE-framework silence tripwire) so they're
    // visible when checking behavior against a real tree.
    println!("--- warnings: {} ---", out.warnings.len());
    for w in &out.warnings {
        println!("  {w}");
    }

    for rid in rule_ids {
        let hits: Vec<_> = out.findings.iter().filter(|f| &f.rule_id == rid).collect();
        println!("--- {rid}: {} ---", hits.len());
        for f in hits.iter().take(5) {
            println!("  {}:{}", f.file, f.line);
        }
    }

    let yarn_findings: Vec<_> = out
        .findings
        .iter()
        .filter(|f| f.file.contains(".yarn/"))
        .collect();
    println!(
        "--- .yarn/* findings (any rule): {} ---",
        yarn_findings.len()
    );
    for f in yarn_findings.iter().take(5) {
        println!("  {} {}:{}", f.rule_id, f.file, f.line);
    }

    let constants_env: usize = out
        .findings
        .iter()
        .filter(|f| {
            f.rule_id == "be-reliability/env-outside-config" && f.file.contains("constants.ts")
        })
        .count();
    println!("--- env-outside-config findings in any constants.ts file: {constants_env} ---");
}
