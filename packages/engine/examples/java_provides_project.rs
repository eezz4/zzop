//! Ad hoc verification harness for `zpz_parser_java::extract_http_provides_project` (the
//! whole-project Spring HTTP route pass — see that module's doc). Not wired into the fused per-file
//! engine pipeline (see `project.rs`), so this walks a `.java` tree directly and calls the parser
//! crate's entry point itself, rather than going through `analyze_tree`.
//!
//! Usage: `cargo run --release -p zpz-engine --example java_provides_project -- <root> [sample_n]`

use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: java_provides_project <root> [sample_n]");
        std::process::exit(2);
    }
    let root = PathBuf::from(&args[0]);
    let sample_n: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(10);

    let mut files: Vec<(String, String)> = Vec::new();
    let mut all_dirs = vec![root.clone()];
    let mut i = 0;
    while i < all_dirs.len() {
        let dir = all_dirs[i].clone();
        i += 1;
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                all_dirs.push(path);
            } else if path.extension().and_then(|e| e.to_str()) == Some("java") {
                if let Ok(text) = std::fs::read_to_string(&path) {
                    let rel = path
                        .strip_prefix(&root)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .replace('\\', "/");
                    files.push((rel, text));
                }
            }
        }
    }
    eprintln!("java files scanned: {}", files.len());

    let report = zpz_parser_java::extract_http_provides_project(&files);
    println!("provides_total={}", report.provides.len());
    println!(
        "skipped_unresolved_prefix={}",
        report.skipped_unresolved_prefix
    );
    println!(
        "skipped_ambiguous_class_name={}",
        report.skipped_ambiguous_class_name
    );

    let mut keys: Vec<String> = report
        .provides
        .iter()
        .map(|p| format!("{} ({}:{})", p.key, p.file, p.line))
        .collect();
    keys.sort();
    println!("--- sample ({sample_n}) ---");
    for k in keys.iter().take(sample_n) {
        println!("  {k}");
    }
}
