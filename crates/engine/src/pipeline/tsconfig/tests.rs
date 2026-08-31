//! `tsconfig_scan` collection tests: compilerOptions extraction, the `extends` merge, the
//! `references` merge (order, conflicts, rebasing, bounds), and the degrade-never-panic paths.

use super::*;
use crate::pipeline::testutil::TempDir;

#[test]
fn tsconfig_scan_collects_star_pattern_and_base_url() {
    let dir = TempDir::new("zzop-tsconfig-star");
    dir.write(
        "tsconfig.json",
        r#"{"compilerOptions": {"baseUrl": ".", "paths": {"@/*": ["./src/*"]}}}"#,
    );
    let scan = tsconfig_scan(dir.path(), std::iter::once("tsconfig.json".to_string()));
    let cfg = scan.get("").unwrap();
    assert_eq!(cfg.base_url, "");
    assert_eq!(cfg.paths.get("@/*").unwrap(), &vec!["./src/*".to_string()]);
}

#[test]
fn tsconfig_scan_registers_under_own_directory_not_root() {
    let dir = TempDir::new("zzop-tsconfig-nested-dir");
    dir.write(
        "packages/app/tsconfig.json",
        r#"{"compilerOptions": {"baseUrl": "src"}}"#,
    );
    let scan = tsconfig_scan(
        dir.path(),
        std::iter::once("packages/app/tsconfig.json".to_string()),
    );
    assert!(scan.contains_key("packages/app"));
    assert_eq!(
        scan.get("packages/app").unwrap().base_url,
        "packages/app/src"
    );
}

#[test]
fn tsconfig_scan_follows_one_level_of_local_extends_and_merges() {
    let dir = TempDir::new("zzop-tsconfig-extends");
    dir.write(
        "tsconfig.base.json",
        r#"{"compilerOptions": {"baseUrl": ".", "paths": {"@shared/*": ["./shared/*"], "@app/*": ["./old-app/*"]}}}"#,
    );
    dir.write(
        "tsconfig.json",
        r#"{"extends": "./tsconfig.base.json", "compilerOptions": {"paths": {"@app/*": ["./src/*"]}}}"#,
    );
    let scan = tsconfig_scan(
        dir.path(),
        vec![
            "tsconfig.json".to_string(),
            "tsconfig.base.json".to_string(),
        ]
        .into_iter(),
    );
    let cfg = scan.get("").unwrap();
    // Child's `@app/*` overrides the parent's; parent-only `@shared/*` is kept (parent-fills-gaps); the
    // parent's `baseUrl` is inherited since the child doesn't declare its own.
    assert_eq!(
        cfg.paths.get("@app/*").unwrap(),
        &vec!["./src/*".to_string()]
    );
    assert_eq!(
        cfg.paths.get("@shared/*").unwrap(),
        &vec!["./shared/*".to_string()]
    );
    assert_eq!(cfg.base_url, "");
}

#[test]
fn tsconfig_scan_ignores_non_local_extends() {
    let dir = TempDir::new("zzop-tsconfig-extends-pkg");
    dir.write(
        "tsconfig.json",
        r#"{"extends": "@tsconfig/node18/tsconfig.json", "compilerOptions": {"paths": {"@/*": ["./src/*"]}}}"#,
    );
    let scan = tsconfig_scan(dir.path(), std::iter::once("tsconfig.json".to_string()));
    // The non-local `extends` target is never read (no such file exists here); the tsconfig's own
    // `compilerOptions` still register normally.
    let cfg = scan.get("").unwrap();
    assert_eq!(cfg.paths.get("@/*").unwrap(), &vec!["./src/*".to_string()]);
}

// --- project `references` ---
//
// Only the SOLUTION file is handed to `tsconfig_scan` in most of these, mirroring the real call: the
// referenced configs are read off disk by path, exactly as `extends` targets already are.

#[test]
fn tsconfig_scan_follows_references_to_collect_paths() {
    // The `npm create vite@latest` shape: a `"files": []` solution file whose compilerOptions live
    // entirely in a referenced config.
    let dir = TempDir::new("zzop-tsconfig-references");
    dir.write(
        "web/tsconfig.json",
        r#"{"files": [], "references": [{"path": "./tsconfig.app.json"}, {"path": "./tsconfig.node.json"}]}"#,
    );
    dir.write(
        "web/tsconfig.app.json",
        r#"{"compilerOptions": {"paths": {"@/*": ["./src/*"]}}, "include": ["src"]}"#,
    );
    dir.write(
        "web/tsconfig.node.json",
        r#"{"compilerOptions": {"strict": true}, "include": ["vite.config.ts"]}"#,
    );
    let scan = tsconfig_scan(dir.path(), std::iter::once("web/tsconfig.json".to_string()));
    let cfg = scan
        .get("web")
        .expect("web must be registered via its references");
    assert_eq!(cfg.paths.get("@/*").unwrap(), &vec!["./src/*".to_string()]);
    // No config in the chain declares `baseUrl`, so the solution file's own directory is the base —
    // the same convention a paths-only tsconfig already gets.
    assert_eq!(cfg.base_url, "web");
}

#[test]
fn tsconfig_scan_reference_conflict_resolves_to_first_declared() {
    // Two referenced configs claiming the same alias. The rule is declaration order in `references`,
    // which is a byte-order property of the file and therefore stable across runs.
    let dir = TempDir::new("zzop-tsconfig-ref-conflict");
    dir.write(
        "tsconfig.json",
        r#"{"references": [{"path": "./tsconfig.app.json"}, {"path": "./tsconfig.node.json"}]}"#,
    );
    dir.write(
        "tsconfig.app.json",
        r#"{"compilerOptions": {"paths": {"@/*": ["./app/*"]}}}"#,
    );
    dir.write(
        "tsconfig.node.json",
        r#"{"compilerOptions": {"paths": {"@/*": ["./node/*"], "@n/*": ["./n/*"]}}}"#,
    );
    let scan = tsconfig_scan(dir.path(), std::iter::once("tsconfig.json".to_string()));
    let cfg = scan.get("").unwrap();
    assert_eq!(cfg.paths.get("@/*").unwrap(), &vec!["./app/*".to_string()]);
    // Non-conflicting keys from the later reference are still collected.
    assert_eq!(cfg.paths.get("@n/*").unwrap(), &vec!["./n/*".to_string()]);
}

#[test]
fn tsconfig_scan_own_paths_outrank_a_reference() {
    let dir = TempDir::new("zzop-tsconfig-ref-own-wins");
    dir.write(
        "tsconfig.json",
        r#"{"compilerOptions": {"paths": {"@/*": ["./mine/*"]}}, "references": [{"path": "./tsconfig.app.json"}]}"#,
    );
    dir.write(
        "tsconfig.app.json",
        r#"{"compilerOptions": {"paths": {"@/*": ["./theirs/*"]}}}"#,
    );
    let scan = tsconfig_scan(dir.path(), std::iter::once("tsconfig.json".to_string()));
    assert_eq!(
        scan.get("").unwrap().paths.get("@/*").unwrap(),
        &vec!["./mine/*".to_string()]
    );
}

#[test]
fn tsconfig_scan_rebases_a_reference_targets_against_its_own_base_url() {
    // The referenced config's `paths` targets are relative to ITS `baseUrl`, but the merged entry has
    // exactly one base. Targets must be rewritten into the entry's base or they resolve to the wrong
    // directory — here `*` under base `web/src` has to become `src/*` under base `web`.
    let dir = TempDir::new("zzop-tsconfig-ref-rebase");
    dir.write(
        "web/tsconfig.json",
        r#"{"files": [], "references": [{"path": "./tsconfig.app.json"}]}"#,
    );
    dir.write(
        "web/tsconfig.app.json",
        r#"{"compilerOptions": {"baseUrl": "./src", "paths": {"@/*": ["*"]}}}"#,
    );
    let scan = tsconfig_scan(dir.path(), std::iter::once("web/tsconfig.json".to_string()));
    let cfg = scan.get("web").unwrap();
    // The reference declared the only `baseUrl` in the chain, so the entry adopts it and the target
    // needs no rewriting.
    assert_eq!(cfg.base_url, "web/src");
    assert_eq!(cfg.paths.get("@/*").unwrap(), &vec!["*".to_string()]);
}

#[test]
fn tsconfig_scan_rebases_the_entrys_own_targets_when_it_adopts_a_references_base_url() {
    // The third frame combination, and the one that regressed: the entry carries `paths` of its own but
    // NO `baseUrl`, so the merged `base_url` is ADOPTED from a reference — and the entry's own targets,
    // written against its own directory, are then in the wrong frame. Before `references` were followed
    // at all, this entry kept `base_url == ""` and its verbatim targets were correct; following
    // references without rewriting them turned a resolving alias into a dead one.
    let dir = TempDir::new("zzop-tsconfig-own-rebase");
    dir.write(
        "tsconfig.json",
        r#"{"compilerOptions": {"paths": {"@app/*": ["./apps/web/src/*"]}},
            "references": [{"path": "./packages/lib"}]}"#,
    );
    dir.write(
        "packages/lib/tsconfig.json",
        r#"{"compilerOptions": {"baseUrl": "src", "paths": {"@lib/*": ["*"]}}}"#,
    );
    let scan = tsconfig_scan(dir.path(), std::iter::once("tsconfig.json".to_string()));
    let cfg = scan.get("").unwrap();
    assert_eq!(cfg.base_url, "packages/lib/src");
    // Written against the root, read against `packages/lib/src` — so it has to climb back out.
    assert_eq!(
        cfg.paths.get("@app/*").unwrap(),
        &vec!["../../../apps/web/src/*".to_string()],
        "the entry's own target must be rewritten into the base it adopted, not carried over verbatim"
    );
    // The reference's own target still rides in the frame it was written against, unchanged.
    assert_eq!(cfg.paths.get("@lib/*").unwrap(), &vec!["*".to_string()]);
}

#[test]
fn tsconfig_scan_rebases_a_reference_when_the_entry_declares_the_base_url() {
    // Same wrinkle, other direction: the entry's own `baseUrl` wins, so the reference's targets — which
    // were written against `web/src` — must be rewritten relative to `web`.
    let dir = TempDir::new("zzop-tsconfig-ref-rebase-entry");
    dir.write(
        "web/tsconfig.json",
        r#"{"compilerOptions": {"baseUrl": "."}, "references": [{"path": "./tsconfig.app.json"}]}"#,
    );
    dir.write(
        "web/tsconfig.app.json",
        r#"{"compilerOptions": {"baseUrl": "./src", "paths": {"@/*": ["*"]}}}"#,
    );
    let scan = tsconfig_scan(dir.path(), std::iter::once("web/tsconfig.json".to_string()));
    let cfg = scan.get("web").unwrap();
    assert_eq!(cfg.base_url, "web");
    assert_eq!(cfg.paths.get("@/*").unwrap(), &vec!["src/*".to_string()]);
}

#[test]
fn tsconfig_scan_reference_to_a_directory_reads_its_tsconfig_json() {
    // TS accepts a directory in `references[].path` and looks for `tsconfig.json` inside it — a
    // different rule from `extends`, which appends `.json` to an extensionless path.
    let dir = TempDir::new("zzop-tsconfig-ref-dir");
    dir.write(
        "tsconfig.json",
        r#"{"files": [], "references": [{"path": "./packages/app"}]}"#,
    );
    dir.write(
        "packages/app/tsconfig.json",
        r#"{"compilerOptions": {"paths": {"@/*": ["./src/*"]}}}"#,
    );
    let scan = tsconfig_scan(dir.path(), std::iter::once("tsconfig.json".to_string()));
    // The referenced config's own base is `packages/app`; merged into the root entry (base ""), its
    // target has to carry that prefix.
    assert_eq!(
        scan.get("").unwrap().paths.get("@/*").unwrap(),
        &vec!["packages/app/src/*".to_string()]
    );
}

#[test]
fn tsconfig_scan_reference_chain_is_followed_exactly_one_level() {
    // A referenced config's OWN `references` are not chased, the same bound `extends` already has.
    // Asserted so the bound is a decision on the record rather than an accident.
    let dir = TempDir::new("zzop-tsconfig-ref-depth");
    dir.write(
        "tsconfig.json",
        r#"{"references": [{"path": "./mid.json"}]}"#,
    );
    dir.write(
        "mid.json",
        r#"{"references": [{"path": "./deep.json"}], "compilerOptions": {"paths": {"@mid/*": ["./mid/*"]}}}"#,
    );
    dir.write(
        "deep.json",
        r#"{"compilerOptions": {"paths": {"@deep/*": ["./deep/*"]}}}"#,
    );
    let scan = tsconfig_scan(dir.path(), std::iter::once("tsconfig.json".to_string()));
    let cfg = scan.get("").unwrap();
    assert!(cfg.paths.contains_key("@mid/*"));
    assert!(!cfg.paths.contains_key("@deep/*"));
}

#[test]
fn tsconfig_scan_ignores_a_reference_escaping_the_analysis_root() {
    let dir = TempDir::new("zzop-tsconfig-ref-escape");
    dir.write(
        "tsconfig.json",
        r#"{"references": [{"path": "../outside/tsconfig.json"}, {"path": "/abs/tsconfig.json"}]}"#,
    );
    let scan = tsconfig_scan(dir.path(), std::iter::once("tsconfig.json".to_string()));
    // Nothing readable, nothing declared -> the directory is not registered at all.
    assert!(scan.is_empty());
}

#[test]
fn tsconfig_scan_tolerates_a_self_referencing_config() {
    let dir = TempDir::new("zzop-tsconfig-ref-self");
    dir.write(
        "tsconfig.json",
        r#"{"compilerOptions": {"paths": {"@/*": ["./src/*"]}}, "references": [{"path": "./tsconfig.json"}]}"#,
    );
    let scan = tsconfig_scan(dir.path(), std::iter::once("tsconfig.json".to_string()));
    assert_eq!(
        scan.get("").unwrap().paths.get("@/*").unwrap(),
        &vec!["./src/*".to_string()]
    );
}

#[test]
fn tsconfig_scan_reference_merge_is_order_stable_across_repeated_scans() {
    // Determinism is a shipped contract: the merged map must be byte-identical run to run.
    let dir = TempDir::new("zzop-tsconfig-ref-determinism");
    dir.write(
        "tsconfig.json",
        r#"{"references": [{"path": "./a.json"}, {"path": "./b.json"}, {"path": "./c.json"}]}"#,
    );
    dir.write(
        "a.json",
        r#"{"compilerOptions": {"paths": {"@x/*": ["./a/*"]}}}"#,
    );
    dir.write(
        "b.json",
        r#"{"compilerOptions": {"paths": {"@x/*": ["./b/*"], "@y/*": ["./b/*"]}}}"#,
    );
    dir.write(
        "c.json",
        r#"{"compilerOptions": {"paths": {"@z/*": ["./c/*"]}}}"#,
    );
    let once = || tsconfig_scan(dir.path(), std::iter::once("tsconfig.json".to_string()));
    let first = once();
    for _ in 0..5 {
        assert_eq!(once(), first);
    }
    assert_eq!(
        first.get("").unwrap().paths.get("@x/*").unwrap(),
        &vec!["./a/*".to_string()]
    );
}

#[test]
fn tsconfig_scan_tolerates_jsonc_comments_and_trailing_commas() {
    let dir = TempDir::new("zzop-tsconfig-jsonc");
    dir.write(
        "tsconfig.json",
        r#"{
            // line comment
            "compilerOptions": {
                /* block comment */
                "baseUrl": ".",
                "paths": {
                    "@/*": ["./src/*"],
                },
            },
        }"#,
    );
    let scan = tsconfig_scan(dir.path(), std::iter::once("tsconfig.json".to_string()));
    let cfg = scan.get("").unwrap();
    assert_eq!(cfg.paths.get("@/*").unwrap(), &vec!["./src/*".to_string()]);
}

#[test]
fn tsconfig_scan_skips_directory_with_neither_base_url_nor_paths() {
    let dir = TempDir::new("zzop-tsconfig-empty");
    dir.write("tsconfig.json", r#"{"compilerOptions": {"strict": true}}"#);
    let scan = tsconfig_scan(dir.path(), std::iter::once("tsconfig.json".to_string()));
    assert!(scan.is_empty());
}

#[test]
fn tsconfig_scan_degrades_on_invalid_json() {
    let dir = TempDir::new("zzop-tsconfig-invalid");
    dir.write("tsconfig.json", "{ this is not json");
    let scan = tsconfig_scan(dir.path(), std::iter::once("tsconfig.json".to_string()));
    assert!(scan.is_empty());
}
