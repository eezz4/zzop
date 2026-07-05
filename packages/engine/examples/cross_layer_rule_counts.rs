//! Ad hoc cross-layer harness — runs `analyze_trees` over 1+ roots and prints, per `cross-layer/*`
//! rule id, the finding count plus up to 5 `file:line` samples, with the join's bucket sizes for
//! context. The multi-tree counterpart to `corpus_rule_counts` (single-root, no `crossLayerFindings`).
//!
//! Usage: `cargo run --release -p zzop-engine --example cross_layer_rule_counts -- <root> [<root> ...]`
//! Each root's folder name becomes its `source_id`. DSL packs are NOT loaded — every `cross-layer/*`
//! rule is native, so skipping packs keeps large runs fast without changing the reported numbers.

use std::collections::BTreeMap;
use std::path::PathBuf;

use zzop_engine::{analyze_trees, EngineConfig};

fn main() {
    let roots: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: cross_layer_rule_counts <root> [<root> ...]");
        std::process::exit(2);
    }

    let trees: Vec<(PathBuf, EngineConfig)> = roots
        .iter()
        .map(|root| {
            let source_id = root
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| root.display().to_string());
            (
                root.clone(),
                EngineConfig {
                    source_id,
                    ..EngineConfig::default()
                },
            )
        })
        .collect();

    let out = analyze_trees(&trees);

    for (root, source, tree_out) in &out.trees {
        eprintln!(
            "tree {source} ({}): files={}",
            root.display(),
            tree_out.file_count
        );
    }

    let cl = &out.cross_layer;
    println!(
        "buckets: edges={} unconsumedProvides={} unprovidedConsumes={} unresolvedConsumes={} externalConsumes={} ambiguousConsumes={}",
        cl.edges.len(),
        cl.unconsumed_provides.len(),
        cl.unprovided_consumes.len(),
        cl.unresolved_consumes.len(),
        cl.external_consumes.len(),
        cl.ambiguous_consumes.len()
    );

    let mut by_rule: BTreeMap<&str, Vec<&zzop_core::Finding>> = BTreeMap::new();
    for f in &out.cross_layer_findings {
        by_rule.entry(f.rule_id.as_str()).or_default().push(f);
    }
    println!(
        "crossLayerFindings total={}",
        out.cross_layer_findings.len()
    );
    // ZZOP_DUMP_MESSAGES=<n>: print the first n full finding messages per rule (useful for checking
    // message wording, not just counts). Off by default to keep runs terse.
    let dump: usize = std::env::var("ZZOP_DUMP_MESSAGES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    for (rule, findings) in &by_rule {
        println!("{rule}: {}", findings.len());
        for f in findings.iter().take(5) {
            println!("  {}:{}", f.file, f.line);
        }
        for f in findings.iter().take(dump) {
            println!("--- {} @ {}:{}", rule, f.file, f.line);
            println!("{}", f.message);
        }
    }
}
