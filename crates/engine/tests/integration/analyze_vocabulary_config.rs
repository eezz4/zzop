//! End-to-end coverage for `EngineConfig::vocabulary` — the convention-vocabulary keys a
//! `zzop.config.jsonc` declares under `vocabulary.*`.
//!
//! Every test here is an INVALIDATION test, and that is the point: a declarable knob whose declaration
//! changes nothing is worse than no knob at all, because the config file then advertises control the
//! engine does not honor. So each case declares a vocabulary and asserts the FINDINGS (or the walked file
//! set) move, against the same fixture that produces the built-in answer.
//!
//! Same `TempDir` fixture-tree pattern as `analyze_io_natives.rs`, whose `mutating-route-no-auth` fixture
//! shape these reuse.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_engine::{analyze_tree, AnalyzeOutput, EngineConfig, VocabularyConfig};

struct TempDir(PathBuf);

impl TempDir {
    fn new(prefix: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("{prefix}-{}-{nanos}-{n}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn write(&self, rel: &str, content: &str) {
        let full = self.0.join(rel);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(full, content).unwrap();
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn scan(dir: &TempDir, vocabulary: VocabularyConfig) -> AnalyzeOutput {
    analyze_tree(
        dir.path(),
        &EngineConfig {
            vocabulary,
            ..EngineConfig::default()
        },
    )
}

fn count(out: &AnalyzeOutput, rule: &str) -> usize {
    out.findings.iter().filter(|f| f.rule_id == rule).count()
}

/// `POST /users` -> `createUser`, unguarded. `DELETE /users/:id` -> `deleteUserGuarded`, which calls
/// `requireAuth` — a name the BUILT-IN guard vocabulary recognizes and a narrowed one does not.
fn guard_fixture() -> TempDir {
    let dir = TempDir::new("zzop-vocab-guard");
    dir.write(
        "routes/api.ts",
        "const apiRoutes = new Hono();\napiRoutes.post(\"/users\", createUser);\napiRoutes.delete(\"/users/:id\", deleteUserGuarded);\n",
    );
    dir.write(
        "routes/handlers.ts",
        "export function createUser(c) {\n  return prisma.user.create({ data: {} });\n}\n\nexport function deleteUserGuarded(c) {\n  requireAuth(c);\n  return prisma.user.delete({ where: { id: c.id } });\n}\n\nexport function requireAuth(c) {\n  return true;\n}\n",
    );
    dir
}

/// Seals that `vocabulary.authGuardPattern` really governs the guard-name check: a declaration that does
/// not cover this project's `requireAuth` must stop clearing the guarded route, so the finding count
/// RISES. Declaring nothing keeps the built-in answer.
#[test]
fn a_narrowed_auth_guard_pattern_stops_clearing_the_guarded_route() {
    let dir = guard_fixture();
    let built_in = scan(&dir, VocabularyConfig::built_in());
    assert_eq!(
        count(&built_in, "mutating-route-no-auth"),
        1,
        "{:?}",
        built_in.findings
    );

    let narrowed = scan(
        &dir,
        VocabularyConfig {
            auth_guard_pattern: Some("(?i)ensureSession".to_string()),
            ..VocabularyConfig::built_in()
        },
    );
    assert_eq!(
        count(&narrowed, "mutating-route-no-auth"),
        2,
        "declaring a guard vocabulary that does not name `requireAuth` must un-clear the DELETE: {:?}",
        narrowed.findings
    );
}

/// Seals what an unusable declaration does — and, since 2026-07-27, what it does NOT do.
///
/// A pattern zzop cannot compile is treated as no declaration at all: no name proves a guard, so both
/// routes report (2). It must not panic (a config file is hand-written, so a malformed regex is a real
/// input), must not match everything, and — the part that changed — must not quietly become zzop's own
/// pattern. Falling back would hide the typo behind an answer the author never wrote, which is the exact
/// substitution this whole axis removed.
#[test]
fn an_uncompilable_auth_guard_pattern_makes_no_guard_judgment_at_all() {
    let dir = guard_fixture();
    let broken = scan(
        &dir,
        VocabularyConfig {
            auth_guard_pattern: Some("(unclosed".to_string()),
            ..VocabularyConfig::built_in()
        },
    );
    assert_eq!(
        count(&broken, "mutating-route-no-auth"),
        2,
        "an unusable pattern must clear nothing, not fall back to zzop's: {:?}",
        broken.findings
    );
    // The same count an author gets by leaving the key out entirely — one contract, two spellings.
    let undeclared = scan(
        &dir,
        VocabularyConfig {
            auth_guard_pattern: None,
            ..VocabularyConfig::built_in()
        },
    );
    assert_eq!(count(&undeclared, "mutating-route-no-auth"), 2);
}

/// Seals `vocabulary.authAcquisitionStandalonePattern`: the auth-acquisition surface is exempt from the
/// rule entirely, so a declaration that no longer calls `/auth/` acquisition must make that route fire.
#[test]
fn a_narrowed_auth_acquisition_vocabulary_un_exempts_the_login_route() {
    let dir = TempDir::new("zzop-vocab-acquisition");
    dir.write(
        "routes/api.ts",
        "const apiRoutes = new Hono();\napiRoutes.post(\"/api/auth/login\", loginUser);\n",
    );
    dir.write(
        "routes/handlers.ts",
        "export function loginUser(c) {\n  return prisma.user.create({ data: {} });\n}\n",
    );

    let built_in = scan(&dir, VocabularyConfig::built_in());
    assert_eq!(
        count(&built_in, "mutating-route-no-auth"),
        0,
        "{:?}",
        built_in.findings
    );

    let narrowed = scan(
        &dir,
        VocabularyConfig {
            // This project spells its acquisition surface `/sessions`, not `/auth` or `/login`.
            auth_acquisition_standalone_pattern: Some("(?i)/(sessions)(/|$)".to_string()),
            ..VocabularyConfig::built_in()
        },
    );
    assert_eq!(
        count(&narrowed, "mutating-route-no-auth"),
        1,
        "a declared acquisition surface that omits /auth must stop exempting it: {:?}",
        narrowed.findings
    );
}

/// Seals `vocabulary.apiSegmentPattern`: the `json` asset veto is INVERTED-gated on an API-ish segment, so
/// a project whose API lives under `/svc/` must be able to say so and get its unmatched consume reported.
#[test]
fn a_declared_api_segment_vocabulary_lifts_the_json_asset_veto() {
    let dir = TempDir::new("zzop-vocab-api-segment");
    dir.write(
        "server/routes.ts",
        "const apiRoutes = new Hono();\napiRoutes.get(\"/svc/health\", health);\n",
    );
    dir.write(
        "client/api.ts",
        "export async function load() {\n  return fetch(\"/svc/users.json\");\n}\n",
    );

    let built_in = scan(&dir, VocabularyConfig::built_in());
    assert_eq!(
        count(&built_in, "unprovided-consume"),
        0,
        "with the built-in vocabulary /svc/ is not API-ish, so the .json consume is vetoed: {:?}",
        built_in.findings
    );

    let declared = scan(
        &dir,
        VocabularyConfig {
            api_segment_pattern: Some("(?i)/(svc)(/|$)".to_string()),
            ..VocabularyConfig::built_in()
        },
    );
    assert_eq!(
        count(&declared, "unprovided-consume"),
        1,
        "declaring /svc/ as this project's API segment must lift the json veto: {:?}",
        declared.findings
    );
}

/// Seals that `vocabulary.skipDirs` reaches the walker. It rides `DispatchConfig::skip_dirs` rather than
/// the vocabulary struct (one list, one owner), so this pins the SEAM: a `vendored/` directory a project
/// names itself must be able to disappear from the analyzed file set.
#[test]
fn declared_skip_dirs_remove_a_directory_from_the_walked_tree() {
    let dir = TempDir::new("zzop-vocab-skip-dirs");
    dir.write("src/app.ts", "export const a = 1;\n");
    dir.write("vendored/lib.ts", "export const b = 2;\n");

    let built_in = scan(&dir, VocabularyConfig::built_in());
    assert_eq!(
        built_in.file_count, 2,
        "the built-in skip list does not know `vendored/`"
    );

    let declared = analyze_tree(
        dir.path(),
        &EngineConfig {
            dispatch: zzop_engine::DispatchConfig {
                skip_dirs: vec!["vendored".to_string()],
                ..zzop_engine::DispatchConfig::default()
            },
            ..EngineConfig::default()
        },
    );
    assert_eq!(
        declared.file_count, 1,
        "a declared skip list must replace the built-in whole, not merge with it"
    );
}

/// The other half of the test above: a prune that changes the answer must SAY it did. Until 2026-08-16
/// `skipDirs` was the only filter in the config with zero trace in the output — the reply for a tree
/// whose sources all sat under a skipped name was indistinguishable from the reply for a tree that
/// genuinely held nothing, which is the one place the tool's central promise (never claim an absence it
/// did not establish) broke under the vocabulary `zzop init` itself writes.
///
/// Asserts the three things a reader has to act on: the directory NAME (the literal string to remove
/// from the config), the config KEY that did it, and the fact that the prune happened before reading —
/// not merely that some warning exists.
#[test]
fn a_skipped_directory_is_disclosed_by_name_and_by_config_key() {
    let dir = TempDir::new("zzop-vocab-skip-disclose");
    dir.write("src/app.ts", "export const a = 1;\n");
    dir.write("build/generated.ts", "export const b = 2;\n");
    dir.write("build/nested/deep.ts", "export const c = 3;\n");

    let skipping = analyze_tree(
        dir.path(),
        &EngineConfig {
            dispatch: zzop_engine::DispatchConfig {
                skip_dirs: vec!["build".to_string()],
                ..zzop_engine::DispatchConfig::default()
            },
            ..EngineConfig::default()
        },
    );
    assert_eq!(skipping.file_count, 1, "the fixture must actually prune");
    let disclosure = skipping
        .warnings
        .iter()
        .find(|w| w.contains("skipDirs"))
        .unwrap_or_else(|| {
            panic!(
                "a prune that removed 2 of 3 files must self-report: {:?}",
                skipping.warnings
            )
        });
    assert!(
        disclosure.contains("build"),
        "the disclosure must name the pruned directory, since that name is the config edit: {disclosure}"
    );
    assert!(
        disclosure.contains("vocabulary.skipDirs"),
        "the disclosure must name the key that did it: {disclosure}"
    );
    // The nested directory is BELOW a pruned one, so the walk never reached it: reporting it would
    // claim a second independent prune that never happened.
    assert!(
        !disclosure.contains("build/nested"),
        "only the pruned directory itself is walked and reported: {disclosure}"
    );

    // Silence when nothing was pruned, or the reader learns to skip the line. An EMPTY skip list is the
    // shape a config-file run reaches with `vocabulary.skipDirs` undeclared (`DispatchConfig::default()`
    // is the Rust-API default and carries the built-in list, which would prune `build/` here too).
    let untouched = analyze_tree(
        dir.path(),
        &EngineConfig {
            dispatch: zzop_engine::DispatchConfig {
                skip_dirs: Vec::new(),
                ..zzop_engine::DispatchConfig::default()
            },
            ..EngineConfig::default()
        },
    );
    assert_eq!(untouched.file_count, 3, "nothing declared, nothing pruned");
    assert!(
        !untouched.warnings.iter().any(|w| w.contains("skipDirs")),
        "an undeclared skip list prunes nothing and must say nothing: {:?}",
        untouched.warnings
    );
}

/// What keeps the disclosure above from being noise, pinned as behaviour because the reasoning in
/// `skipped_dirs_warning`'s doc rests on it: a directory a COMMITTED `.gitignore` already excludes never
/// reaches the skip-list prune, so the shipped template's `node_modules`/`dist`/`target` — gitignored in
/// virtually every tree they appear in — cost nothing in the line, and what survives to be named is the
/// COMMITTED directory that was skipped anyway. That is the interesting case by construction. Measured on
/// this repo the same day: 11 declared names, 2 reported.
///
/// If this ever inverts (the prune running ahead of the ignore matcher), the disclosure does not become
/// wrong — it becomes loud, which is its own way of going unread. That is the failure this test catches.
#[test]
fn a_gitignored_directory_is_not_attributed_to_the_skip_list() {
    let dir = TempDir::new("zzop-vocab-skip-gitignored");
    dir.write("src/app.ts", "export const a = 1;\n");
    dir.write(".gitignore", "node_modules/\n");
    dir.write("node_modules/pkg/index.ts", "export const b = 2;\n");
    dir.write("build/generated.ts", "export const c = 3;\n");

    let out = analyze_tree(
        dir.path(),
        &EngineConfig {
            dispatch: zzop_engine::DispatchConfig {
                skip_dirs: vec!["node_modules".to_string(), "build".to_string()],
                ..zzop_engine::DispatchConfig::default()
            },
            ..EngineConfig::default()
        },
    );
    let disclosure = out
        .warnings
        .iter()
        .find(|w| w.contains("skipDirs"))
        .unwrap_or_else(|| panic!("`build/` is committed and pruned: {:?}", out.warnings));
    assert!(
        disclosure.contains("build"),
        "the committed-but-skipped directory is the one worth naming: {disclosure}"
    );
    assert!(
        !disclosure.contains("node_modules"),
        "a gitignored directory was already out of the walk — attributing it to the skip list inflates \
         the line with the entries nobody needs to read: {disclosure}"
    );
}

/// The other volume guard, and the one that would otherwise fire on EVERY git repository: `.git` is
/// pruned by the shipped skip list in every tree that has one, and no project can put analyzable source
/// in git's object store. A permanently-present, never-actionable line is how a warning teaches its
/// reader to skip the whole channel — including the `build/` case it exists for. See
/// `config_filters::NOT_SOURCE_BY_CONSTRUCTION` for why this exclusion is drawn on the fact/convention
/// axis and not on "this one looks boring".
#[test]
fn a_machine_owned_directory_is_pruned_but_never_named() {
    let dir = TempDir::new("zzop-vocab-skip-machine-owned");
    dir.write("src/app.ts", "export const a = 1;\n");
    dir.write(".git/config", "[core]\n");
    dir.write("zzop-reports/report.md", "# old\n");

    let only_machine_owned = analyze_tree(
        dir.path(),
        &EngineConfig {
            dispatch: zzop_engine::DispatchConfig {
                skip_dirs: vec![".git".to_string(), "zzop-reports".to_string()],
                ..zzop_engine::DispatchConfig::default()
            },
            ..EngineConfig::default()
        },
    );
    assert!(
        !only_machine_owned
            .warnings
            .iter()
            .any(|w| w.contains("skipDirs")),
        "a run whose only prunes are machine-owned has nothing to disclose: {:?}",
        only_machine_owned.warnings
    );

    // And the filter must not swallow the line when a real directory is pruned alongside them.
    dir.write("build/generated.ts", "export const b = 2;\n");
    let mixed = analyze_tree(
        dir.path(),
        &EngineConfig {
            dispatch: zzop_engine::DispatchConfig {
                skip_dirs: vec![
                    ".git".to_string(),
                    "zzop-reports".to_string(),
                    "build".to_string(),
                ],
                ..zzop_engine::DispatchConfig::default()
            },
            ..EngineConfig::default()
        },
    );
    let disclosure = mixed
        .warnings
        .iter()
        .find(|w| w.contains("skipDirs"))
        .unwrap_or_else(|| panic!("`build/` still needs its line: {:?}", mixed.warnings));
    assert!(
        disclosure.contains("build") && !disclosure.contains(".git"),
        "the reportable prune is named and the machine-owned ones are not: {disclosure}"
    );
    assert!(
        disclosure.contains("1 directory"),
        "the COUNT is over reportable prunes too — counting 3 while naming 1 invites the reader to hunt \
         for two directories the line will never identify: {disclosure}"
    );
}

/// Seals the no-fallback contract in ONE place, over the whole struct: an undeclared vocabulary must NOT
/// produce the same findings as the declared built-in one. Before 2026-07-27 this test asserted the
/// opposite — that the two were byte-identical — which was the fallback's own definition; inverting it is
/// how the reversal stays checked rather than merely described.
///
/// The assertion is deliberately about the whole finding list, not one rule: this is the place a
/// re-introduced fallback ANYWHERE in the struct shows up.
#[test]
fn an_undeclared_vocabulary_is_not_the_built_in_one() {
    let dir = guard_fixture();
    let ids = |o: &AnalyzeOutput| {
        o.findings
            .iter()
            .map(|f| (f.rule_id.clone(), f.file.clone(), f.line))
            .collect::<Vec<_>>()
    };
    let undeclared = scan(&dir, VocabularyConfig::default());
    let declared = scan(&dir, VocabularyConfig::built_in());
    assert_ne!(
        ids(&undeclared),
        ids(&declared),
        "an undeclared vocabulary must not silently behave like the declared built-in one"
    );
    // Concretely, on this fixture: with nothing declared, `requireAuth` is not a guard name, so the
    // route it protects reports alongside the genuinely unguarded one.
    assert_eq!(count(&undeclared, "mutating-route-no-auth"), 2);
    assert_eq!(count(&declared, "mutating-route-no-auth"), 1);
}

/// The measured defect at the level a user meets it: `vocabulary.authGuardPattern` set to an
/// unparseable regex produced `configWarnings: []`, exit 0, and output BYTE-IDENTICAL to the same run
/// with a valid pattern. Every consumer compiles with `Regex::new(..).ok()`, so an uncompilable value
/// collapses to the same `None` an UNDECLARED key has — and undeclared means "make no judgment". The
/// project that took the trouble to declare, and fat-fingered a bracket, got exactly the treatment of
/// the project that declared nothing.
#[test]
fn an_uncompilable_declared_pattern_is_reported_rather_than_swallowed() {
    let dir = TempDir::new("zzop-engine-vocab-uncompilable");
    dir.write("a.ts", "export function noop() { return 1; }\n");

    let cfg = EngineConfig {
        source_id: "t".to_string(),
        vocabulary: zzop_engine::VocabularyConfig {
            auth_guard_pattern: Some("(?i)((((zorp[".to_string()),
            ..zzop_engine::VocabularyConfig::built_in()
        },
        ..EngineConfig::default()
    };
    let out = analyze_tree(dir.path(), &cfg);
    assert!(
        out.warnings
            .iter()
            .any(|w| w.contains("vocabulary.authGuardPattern") && w.contains("made no judgment")),
        "the declaration was ignored and the run must say so: {:?}",
        out.warnings
    );
    // Ignored, not fatal — the bad key costs its own judgment and nothing else.
    assert_eq!(out.file_count, 1);
}

/// The invalidation: a valid vocabulary must add no warning. Without it, "report the uncompilable
/// ones" is indistinguishable from "report every declared pattern".
#[test]
fn a_compilable_declared_pattern_adds_no_warning() {
    let dir = TempDir::new("zzop-engine-vocab-compilable");
    dir.write("a.ts", "export function noop() { return 1; }\n");

    let cfg = EngineConfig {
        source_id: "t".to_string(),
        vocabulary: zzop_engine::VocabularyConfig::built_in(),
        ..EngineConfig::default()
    };
    let out = analyze_tree(dir.path(), &cfg);
    assert!(
        !out.warnings
            .iter()
            .any(|w| w.contains("is not a valid regular expression")),
        "{:?}",
        out.warnings
    );
}
