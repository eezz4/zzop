//! Test-only pin behind `parse::parse_with_cm`'s one-entry memo. Split out of `parse.rs` on
//! 2026-08-08, the batch that added the memo, because that file crossed the line ratchet.

mod no_hygiene_dependency_pin {
    #[test]
    fn this_crate_names_no_swc_hygiene_type() {
        let src_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");

        let mut offenders = Vec::new();
        let mut scanned = 0usize;
        let mut stack = vec![src_root.clone()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).expect("parser-typescript/src must be readable") {
                let path = entry.expect("readable dir entry").path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                    continue;
                }
                scanned += 1;
                // This file IS the pin; its token list necessarily spells what it forbids. One
                // self-exemption, not a growing allowlist — every other file is judged.
                if path.file_name().and_then(|n| n.to_str()) == Some("hygiene_pin_tests.rs") {
                    continue;
                }
                let text = std::fs::read_to_string(&path).expect("readable .rs file");
                // The claim is about CODE, not prose: the docs explaining WHY hygiene is irrelevant
                // here have to name it. Skipping comment lines keeps the rule principled — a real
                // `use swc_core::common::SyntaxContext` still trips it — instead of buying silence
                // with a per-file exemption every time a doc mentions the word.
                for line in text.lines() {
                    let code = line.trim_start();
                    if code.starts_with("//") {
                        continue;
                    }
                    for token in ["SyntaxContext", "Mark::", "::hygiene", "resolver("] {
                        if code.contains(token) {
                            offenders.push(format!("{}: {token}", path.display()));
                        }
                    }
                }
            }
        }
        // Empty-subject floor: a walk that reads nothing would report clean while proving nothing.
        assert!(
            scanned > 1,
            "scanned {scanned} .rs file(s) under {} — an empty subject set is a broken pin, not a \
             clean crate",
            src_root.display()
        );
        assert!(
            offenders.is_empty(),
            "this crate now reads swc hygiene state: {offenders:?}. `parse_with_cm`'s one-entry memo \
             hands back a `Module` parsed under a DIFFERENT `Globals` scope, which is only safe while \
             no `SyntaxContext` is ever read. Either drop the memo or scope the parse and its readers \
             into one `GLOBALS.set` — do not silence this test."
        );
    }
}
