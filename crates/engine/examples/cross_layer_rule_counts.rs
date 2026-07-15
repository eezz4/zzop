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

    let mut trees: Vec<(PathBuf, EngineConfig)> = roots
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

    // ZZOP_OVERLAYS="sourceId=path.json;sourceId2=path2.json": load a Mode B adapter-overlay envelope
    // onto the matching tree's `EngineConfig::adapter_overlays`, for before/after measurement of an
    // out-of-tree adapter (e.g. examples/react-query-adapter) without wiring a dedicated harness per
    // adapter. This is measurement tooling, not a production path — a bad entry fails loud (exit 2)
    // rather than silently running the "before" numbers under an "after" label.
    if let Ok(spec) = std::env::var("ZZOP_OVERLAYS") {
        for entry in spec.split(';').filter(|s| !s.is_empty()) {
            let Some((source_id, path)) = entry.split_once('=') else {
                eprintln!("ZZOP_OVERLAYS: malformed entry {entry:?} (want sourceId=path.json)");
                std::process::exit(2);
            };
            let Some((_, cfg)) = trees.iter_mut().find(|(_, c)| c.source_id == source_id) else {
                eprintln!(
                    "ZZOP_OVERLAYS: unknown sourceId {source_id:?} (no tree with that source_id)"
                );
                std::process::exit(2);
            };
            let text = std::fs::read_to_string(path).unwrap_or_else(|e| {
                eprintln!("ZZOP_OVERLAYS: cannot read {path:?}: {e}");
                std::process::exit(2);
            });
            let envelope: zzop_core::NormalizedEnvelope = serde_json::from_str(&text)
                .unwrap_or_else(|e| {
                    eprintln!("ZZOP_OVERLAYS: cannot parse {path:?} as NormalizedEnvelope: {e}");
                    std::process::exit(2);
                });
            cfg.adapter_overlays.push(envelope);
        }
    }

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

    // ZZOP_DUMP_BUCKETS=1: print every entry of the non-edge buckets (file:line + key/raw), for
    // classifying WHICH call sites landed where during corpus measurement. Off by default.
    if std::env::var("ZZOP_DUMP_BUCKETS").is_ok_and(|v| v == "1") {
        // Coverage/self-report findings live in the PER-TREE findings, not crossLayerFindings —
        // dump them too so a "both trees blind, zero cross findings" run shows whether the
        // honesty channels fired at all.
        for (_, source, tree_out) in &out.trees {
            for w in &tree_out.warnings {
                println!("bucket tree-warning ({source}): {w}");
            }
            for f in tree_out
                .findings
                .iter()
                .filter(|f| f.rule_id.starts_with("coverage") || f.rule_id.contains("unresolved"))
            {
                println!(
                    "bucket tree-coverage ({source}): {} @ {}:{}",
                    f.rule_id, f.file, f.line
                );
            }
        }
        for e in &cl.edges {
            println!(
                "bucket edges: {} {}:{} ({}) -> {}:{} ({})",
                e.key, e.from.file, e.from.line, e.from.source, e.to.file, e.to.line, e.to.source
            );
        }
        for (name, consumes) in [
            ("unprovidedConsumes", &cl.unprovided_consumes),
            ("unresolvedConsumes", &cl.unresolved_consumes),
            ("externalConsumes", &cl.external_consumes),
        ] {
            for c in consumes {
                println!(
                    "bucket {name}: {}:{} key={:?} raw={:?} (source {})",
                    c.consume.file, c.consume.line, c.consume.key, c.consume.raw, c.source
                );
            }
        }
        for p in &cl.unconsumed_provides {
            println!(
                "bucket unconsumedProvides: {}:{} key={} (source {})",
                p.provide.file, p.provide.line, p.provide.key, p.source
            );
        }
    }

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
