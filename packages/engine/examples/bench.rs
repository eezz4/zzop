//! Cold/warm benchmark harness over a real source tree.
//!
//! Usage: `cargo run --release -p zpz-engine --example bench -- <root> [--packs <dir>] [--cache <dir>] [--git]`
//! Runs analyze_tree twice (cold, then warm when a cache dir is given) and prints wall times,
//! file/finding counts, cache stats, and the top-10 slowest rules. Timings are wall-clock; compare
//! orders of magnitude across runs, not exact values.

use std::path::PathBuf;
use std::time::Instant;

use zpz_engine::{analyze_tree, EngineConfig, GitOptions};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(root) = args.first() else {
        eprintln!("usage: bench <root> [--packs <dir>] [--cache <dir>] [--git]");
        std::process::exit(2);
    };
    let root = PathBuf::from(root);
    let mut packs_dir: Option<PathBuf> = None;
    let mut cache_dir: Option<PathBuf> = None;
    let mut git = false;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--packs" => {
                packs_dir = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--cache" => {
                cache_dir = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--git" => {
                git = true;
                i += 1;
            }
            other => {
                eprintln!("unknown arg: {other}");
                std::process::exit(2);
            }
        }
    }

    let mut config = EngineConfig {
        source_id: "bench".to_string(),
        profile_rules: true,
        ..EngineConfig::default()
    };
    if let Some(dir) = &packs_dir {
        let loaded = zpz_core::load_dsl_packs(dir);
        eprintln!(
            "packs: {} loaded, {} errors",
            loaded.packs.len(),
            loaded.errors.len()
        );
        config.packs = loaded.packs.into_iter().map(|(_, p)| p).collect();
    }
    if git {
        config.git = Some(GitOptions::default());
    }
    if let Some(dir) = &cache_dir {
        let _ = std::fs::remove_dir_all(dir); // cold run = empty cache
        config.cache_dir = Some(dir.clone());
    }

    run_once("cold", &root, &config);
    if cache_dir.is_some() {
        run_once("warm", &root, &config);
    }
}

fn run_once(label: &str, root: &std::path::Path, config: &EngineConfig) {
    let start = Instant::now();
    let out = analyze_tree(root, config);
    let elapsed = start.elapsed();
    println!(
        "[{label}] {:.2?}  files={} findings={} degraded={} nodes={} scores={} cache={:?}",
        elapsed,
        out.file_count,
        out.findings.len(),
        out.degraded.len(),
        out.nodes.len(),
        out.scores.is_some(),
        out.cache,
    );
    for w in &out.warnings {
        println!("[{label}] warning: {w}");
    }
    if let Some(timings) = &out.rule_timings {
        println!("[{label}] top rules by time:");
        for t in timings.iter().take(10) {
            println!(
                "  {:<40} {:>8.2}ms  findings={}",
                t.rule_id,
                t.nanos as f64 / 1e6,
                t.findings
            );
        }
    }
}
