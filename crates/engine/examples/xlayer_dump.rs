//! Ad hoc: dump the cross-layer join buckets' CONTENTS (keys), not just counts — to eyeball whether an
//! unprovided consume / unconsumed provide is a real contract gap or an extraction artifact.
//! Usage: `cargo run --release -p zzop-engine --example xlayer_dump -- <root> [<root> ...]`

use std::path::PathBuf;

use zzop_engine::{analyze_trees, EngineConfig};

fn main() {
    let roots: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    let trees: Vec<(PathBuf, EngineConfig)> = roots
        .iter()
        .map(|root| {
            let source_id = root
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
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
    let cl = &out.cross_layer;

    println!("=== edges ({}) ===", cl.edges.len());
    for e in &cl.edges {
        println!("  {}", e.key);
    }
    println!(
        "\n=== unprovided consumes ({}) ===",
        cl.unprovided_consumes.len()
    );
    for c in &cl.unprovided_consumes {
        println!(
            "  {:?}  @ {} {}:{}",
            c.consume.key, c.source, c.consume.file, c.consume.line
        );
    }
    println!(
        "\n=== unconsumed provides ({}) ===",
        cl.unconsumed_provides.len()
    );
    for p in &cl.unconsumed_provides {
        println!(
            "  {:?}  @ {} {}:{}",
            p.provide.key, p.source, p.provide.file, p.provide.line
        );
    }
    println!(
        "\n=== unresolved consumes ({}) ===",
        cl.unresolved_consumes.len()
    );
    for c in &cl.unresolved_consumes {
        println!(
            "  raw={:?} method={:?}  @ {} {}:{}",
            c.consume.raw, c.consume.method, c.source, c.consume.file, c.consume.line
        );
    }
}
