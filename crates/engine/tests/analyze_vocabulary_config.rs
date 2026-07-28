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
