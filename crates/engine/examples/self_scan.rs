//! Ad hoc self-scan harness: run the full engine (all DSL packs + native rules) over one root and
//! print EVERY finding grouped by rule id (count + a few `file:line` samples), plus warnings and the
//! cross-layer bucket sizes. Unlike `corpus_rule_counts` (which counts only rule ids you name), this
//! enumerates whatever fired — the shape you want when dogfooding zzop on its own tree.
//!
//! Usage: `cargo run --release -p zzop-engine --example self_scan -- <root> [<extra_skip_dir> ...]`
//! Extra args after the root are appended to the dispatch skip-dir list (e.g. `corpus` to keep the
//! RealWorld fixtures out of a scan of zzop itself).

use std::collections::BTreeMap;
use std::path::PathBuf;

use zzop_engine::{analyze_tree, EngineConfig};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: self_scan <root> [<extra_skip_dir> ...]");
        std::process::exit(2);
    }
    let root = PathBuf::from(&args[0]);
    let extra_skips = &args[1..];

    let loaded = zzop_core::load_dsl_packs(std::path::Path::new("rules/dsl"));
    eprintln!(
        "packs: {} loaded, {} errors",
        loaded.packs.len(),
        loaded.errors.len()
    );
    for err in &loaded.errors {
        eprintln!("  pack error: {err:?}");
    }

    let mut config = EngineConfig {
        source_id: "zzop".to_string(),
        packs: loaded.packs.into_iter().map(|(_, p)| p).collect(),
        ..EngineConfig::default()
    };
    for d in extra_skips {
        config.dispatch.skip_dirs.push(d.clone());
    }

    let out = analyze_tree(&root, &config);
    println!(
        "files={} findings_total={} warnings={}",
        out.file_count,
        out.findings.len(),
        out.warnings.len()
    );

    // Findings grouped by rule id, most-fired first, then by id for stable ties.
    let mut by_rule: BTreeMap<String, Vec<&zzop_core::Finding>> = BTreeMap::new();
    for f in &out.findings {
        by_rule.entry(f.rule_id.clone()).or_default().push(f);
    }
    let mut rows: Vec<(&String, &Vec<&zzop_core::Finding>)> = by_rule.iter().collect();
    rows.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then(a.0.cmp(b.0)));

    println!("\n=== findings by rule ({} rules fired) ===", rows.len());
    for (rid, hits) in &rows {
        println!("[{:>3}] {}", hits.len(), rid);
        for f in hits.iter().take(4) {
            println!("        {}:{}  {}", f.file, f.line, f.message);
        }
        if hits.len() > 4 {
            println!("        … +{} more", hits.len() - 4);
        }
    }

    println!("\n=== warnings ({}) ===", out.warnings.len());
    for w in &out.warnings {
        println!("  {w}");
    }
}
