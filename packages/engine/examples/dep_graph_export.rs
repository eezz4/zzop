//! Ad hoc dependency-graph exporter — runs `analyze_tree` over one root and prints the file-level
//! dependency graph (`MinimalIr::dep`) in Graphviz DOT or Mermaid format. madge-style visualization
//! without madge: the engine's resolver already handled tsconfig paths/aliases/NodeNext, so the
//! edges are the engine's own truth, not a re-derivation.
//!
//! Usage: `cargo run --release -p zpz-engine --example dep_graph_export -- <root> [dot|mermaid]`
//! Output is deterministic; files with no edges in either direction are omitted for readability.

use std::collections::BTreeSet;
use std::path::PathBuf;

use zpz_engine::{analyze_tree, EngineConfig};

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(root) = args.next().map(PathBuf::from) else {
        eprintln!("usage: dep_graph_export <root> [dot|mermaid]");
        std::process::exit(2);
    };
    let format = args.next().unwrap_or_else(|| "dot".to_string());

    let out = analyze_tree(&root, &EngineConfig::default());
    let dep = &out.ir.ir.dep;

    let mut edges: BTreeSet<(&str, &str)> = BTreeSet::new();
    for (from, tos) in dep.iter() {
        for to in tos {
            edges.insert((from.as_str(), to.as_str()));
        }
    }
    let mut nodes: BTreeSet<&str> = BTreeSet::new();
    for (a, b) in &edges {
        nodes.insert(a);
        nodes.insert(b);
    }
    eprintln!(
        "files={} connected={} edges={}",
        out.file_count,
        nodes.len(),
        edges.len()
    );

    match format.as_str() {
        "mermaid" => {
            println!("flowchart LR");
            for (i, n) in nodes.iter().enumerate() {
                println!("  n{i}[\"{n}\"]");
            }
            let index: Vec<&str> = nodes.iter().copied().collect();
            for (a, b) in &edges {
                let ai = index.binary_search(a).unwrap();
                let bi = index.binary_search(b).unwrap();
                println!("  n{ai} --> n{bi}");
            }
        }
        _ => {
            println!("digraph deps {{");
            println!("  rankdir=LR; node [shape=box, fontsize=10];");
            for n in &nodes {
                println!("  \"{n}\";");
            }
            for (a, b) in &edges {
                println!("  \"{a}\" -> \"{b}\";");
            }
            println!("}}");
        }
    }
}
