//! T2 policy-value equality pins: two constants that live in different crates, encode the *same*
//! policy, and therefore cannot share a symbol (T1) across that crate boundary. A pin test here is
//! the substitute — if one constant changes, this test fails and forces the other to be
//! re-justified rather than silently drifting apart.

// MIN_FOREIGN_UNPROVIDED_GROUP (rules-http) and MIN_PREFIX_DRIFT_GROUP
// (rules-cross-layer) encode the same policy ("2 is coincidence, 3 is a
// pattern") across a crate boundary; if one changes, this pin forces the
// other to be re-justified.
#[test]
fn min_foreign_unprovided_group_matches_min_prefix_drift_group() {
    assert_eq!(
        zzop_rules_http::unprovided_consume::MIN_FOREIGN_UNPROVIDED_GROUP,
        zzop_rules_cross_layer::cross_layer::prefix_drift::MIN_PREFIX_DRIFT_GROUP,
        "MIN_FOREIGN_UNPROVIDED_GROUP (rules-http) and MIN_PREFIX_DRIFT_GROUP (rules-cross-layer) both \
         encode the same \"2 is coincidence, 3+ is a pattern\" fold-threshold policy for aggregating \
         same-cause findings; a crate boundary prevents T1 symbol sharing, so this equality pin is the \
         T2 substitute (rule-quality.md §6) — if one changes, re-justify the other and update this pin."
    );
}

/// The `print_stdout`/`exit` exemption may sit on a target ROOT or on a TEST module — never on a library
/// module.
///
/// Cargo's lint table is per-package, so a library file can opt itself out with a file-level `#![allow]`
/// and clippy stays green: the workspace `warn` is simply overridden for that whole file. That is the
/// hole this guards. The root manifest carried the set as PROSE ("exactly ten entry points") with nothing
/// checking it, which is the shape this repo has been burned by repeatedly — and a count would go stale
/// the next time someone adds a legitimate example. A predicate does not.
///
/// Deliberately NOT asserting a number: adding an example is normal and must not break a test; a library
/// silently uncovering itself is the event worth failing on.
#[test]
fn a_stdout_exemption_sits_on_a_target_root_or_a_test_module_never_on_a_library() {
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("crates/engine sits two levels under the workspace root")
        .to_path_buf();

    let mut offenders: Vec<String> = Vec::new();
    let mut seen = 0usize;
    let mut stack = vec![workspace.clone()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if path.is_dir() {
                // Skip build output and the analysis CORPUS — fixture trees are subjects, not our code.
                if !matches!(
                    name.as_str(),
                    "target" | ".git" | "node_modules" | "cases" | "corpus" | "runs" | "trees"
                ) {
                    stack.push(path);
                }
                continue;
            }
            if !name.ends_with(".rs") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let exempts = text.lines().any(|line| {
                line.starts_with("#![allow(")
                    && (line.contains("print_stdout") || line.contains("clippy::exit"))
            });
            if !exempts {
                continue;
            }
            seen += 1;
            let rel = path
                .strip_prefix(&workspace)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let is_target_root = rel.contains("/examples/")
                || rel.ends_with("/src/main.rs")
                || rel.contains("/src/bin/");
            // 🔴 This asked the FILE NAME and nothing else, and that could not see a whole shape of
            // its own population: a module split into `tests.rs` + `tests/<part>.rs` is test-only by
            // construction, and `tests/census.rs` was judged a library module because of its name
            // (review ledger V139). Same class as V138 — a needle that cannot reach part of the set it
            // claims to cover, reporting what it could not read as a verdict.
            //
            // So the directory case is decided by the DECLARATION chain rather than by the spelling:
            // `<dir>/tests/<part>.rs` counts only when `<dir>/tests.rs` exists (it is the module body)
            // AND `<dir>.rs` declares that module under `#[cfg(test)]`. A directory someone merely
            // NAMED `tests` and wired into the library still fails, which is the point of the pin.
            let is_test_module =
                name.ends_with("_tests.rs") || name == "tests.rs" || declared_under_cfg_test(&path);
            if !is_target_root && !is_test_module {
                offenders.push(rel);
            }
        }
    }

    assert!(
        seen > 0,
        "found no stdout exemptions at all — this test stopped looking at the tree rather than passing"
    );
    assert!(
        offenders.is_empty(),
        "a file-level print_stdout/exit exemption on a LIBRARY module uncovers that whole file while \
         clippy stays green — move the printing into a target root, or make the module test-only: {offenders:?}"
    );
}

/// Whether `path` is a part of a `#[cfg(test)] mod tests;` module body, proven by reading the two
/// files that declare it rather than by trusting the directory's name.
fn declared_under_cfg_test(path: &std::path::Path) -> bool {
    let Some(dir) = path.parent() else {
        return false;
    };
    if dir.file_name().and_then(|n| n.to_str()) != Some("tests") {
        return false;
    }
    // `<dir>/../tests.rs` is the module body that declares this part.
    let body = dir.with_extension("rs");
    let Ok(body_text) = std::fs::read_to_string(&body) else {
        return false;
    };
    let part = path
        .file_stem()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    if !body_text.contains(&format!("mod {part};")) {
        return false;
    }
    // And whoever declares `mod tests;` must mark it test-only.
    let Some(owner) = dir.parent().map(|d| d.with_extension("rs")) else {
        return false;
    };
    let Ok(owner_text) = std::fs::read_to_string(&owner) else {
        return false;
    };
    owner_text.contains("#[cfg(test)]") && owner_text.contains("mod tests;")
}
