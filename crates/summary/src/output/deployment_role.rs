//! DEPLOYMENT ROLE — the OUTERMOST first-screen ordering key, ahead of severity: does this finding's file
//! ship, or is it part of the scaffolding around what ships?
//!
//! ## Why this is an ordering axis and never a filter
//! Every finding here is real and every count includes it. What the axis decides is which 50 of a repo's
//! thousands a reader sees FIRST — and a first screen led by fixtures and build scripts reads as "this
//! tool reports noise", while the identical set led by product code reads as a work list. The axis was
//! built because a blind audit of one large monorepo found the actionable findings ALREADY PRESENT in the
//! first 40 and simply ordered behind the scaffolding; the counts for that run belong to its commit
//! message, not here, where they would be a number nobody can recount.
//!
//! ## The three roles, and why each boundary is a FACT rather than a guess
//! * `SHIPPED` (2) — everything else. The default, so a file nobody can classify is never demoted.
//! * `TEST_PATH` (1) — the DSL's own shared `${test-paths}` fragment (`zzop_facade::test_path_re`). One
//!   owner, so this ordering and the packs' own test-path exclusions cannot disagree about what a test
//!   path is. The 2026-08-09 U78 ruling: the credential rules deliberately keep scanning test paths (a
//!   committed secret is a leak wherever it sits, and the catalog states that policy), but a first run's
//!   screen should not be a wall of `password123` fixtures.
//! * `BUILD_SURFACE` (0) — two independent sources, both statements the ECOSYSTEM or the TREE ITSELF
//!   makes, never a name this crate picked:
//!   1. the tree's own `package.json` `scripts` path tokens, resolved against the walked file set and
//!      carried on the wire as `buildScriptPaths` — the analyzed tree declaring, in its own manifest,
//!      "this file is how I am built, not what I ship";
//!   2. `zzop_facade::build_path_re` — anything under `.github/`, and any `*.example` template: the two
//!      path shapes no project can rename (that predicate's own doc carries why it is a Rust predicate
//!      rather than a shared DSL fragment, why `.github/` is taken whole, and why `scripts/`/`tools/`/
//!      `bin/` are deliberately NOT arms).
//!
//! ## Precedence, and why BUILD sorts BELOW TEST
//! A path can satisfy both lower predicates (a `scripts/seed-fixtures.ts` named by a manifest and sitting
//! under `fixtures/`), so one has to win, and BUILD does.
//!
//! ⚠ The reason is NOT "build reads a declaration where test reads a name" — that is true of the manifest
//! arm and FALSE of the `.github/`/`*.example` arm, which is a name shape exactly like the test predicate.
//! The two build arms genuinely differ in evidential weight and are deliberately collapsed into one role;
//! saying otherwise would claim declaration strength for a regex.
//!
//! What actually justifies last place is the REMEDIATION AUDIENCE, which is the same for both arms and
//! different from test code's. A test file is the product team's, in the product's language, edited in the
//! same review as the code beside it. CI machinery and build scripts belong to whoever owns the pipeline,
//! are usually written in a different language (YAML, shell), and a finding there is frequently not a
//! defect at all but a deliberate fixture of the release process. On a first screen that is the furthest
//! thing from "what should I fix in this codebase today".
//!
//! ## Why role is the OUTER key, ahead of severity (2026-08-25 — this axis's second measurement)
//! It was not. Until 2026-08-25 severity ranked first and role broke ties inside a band, which meant the
//! axis could never touch the one screen it was built for. A blind audit of cal.com's first 40 rows found
//! all SIX `critical` rows were a redaction-test PEM fixture and two build scripts, and read them the way
//! a first-time user does: *"a real user reading from the top and stopping when it stops paying stops at
//! n7 having found nothing — and n7 is the last critical-band item, precisely the psychological place to
//! quit."* Severity-first ordering cannot fix that, because the demotion it allows is bounded by the band:
//! a non-shipped `critical` still outranks every shipped `warning` in the tree.
//!
//! So REACH and RANKING were separated. Reach is unchanged and stays argued in each rule's own prose (an
//! actual PEM header in a fixture IS a committed key, so the credential rules keep scanning test paths and
//! the catalog says so). Ranking is this: a finding in code the tree does not ship sorts below every
//! finding in code it does, whatever band either carries. Inside `SHIPPED`, severity orders exactly as it
//! always did, so a shipped `critical` is still row one overall.
//!
//! ⚠ This is still ORDERING ONLY. No finding's `severity` value moves, `bySeverity` is untouched, and
//! `--fail-on` reads those counts rather than this list — so a CI gate sees nothing here. Whether a
//! non-shipped finding's severity VALUE should also change is a separate, unmade decision, and it is
//! separate precisely because it would move `bySeverity` and therefore that gate.
//!
//! The two disclosure counts beside the reply are computed independently and may therefore overlap —
//! each states a true thing about the full set, and neither is a partition.
//!
//! ## What this axis deliberately cannot see
//! It is a JS/npm ecosystem fact plus two file-name shapes. A tree with no `package.json`, no
//! `.github/` and no `*.example` has no build surface at all, and on such a tree the ordering
//! is byte-identical to what it was before this module existed. Sold as a general axis it would be a lie.
//! To find out for any one tree, run `zzop analyze --config <cfg> --limit 0` and look for
//! `findings.buildPaths`: an absent key IS the "no build surface here" answer.

/// Ships: the default role. Never demoted.
pub(crate) const ROLE_SHIPPED: u8 = 2;
/// A test path by the shared `${test-paths}` vocabulary.
pub(crate) const ROLE_TEST_PATH: u8 = 1;
/// Build/release surface — manifest-declared script, `.github/` machinery, or `*.example` template.
pub(crate) const ROLE_BUILD_SURFACE: u8 = 0;

/// The finding's file, or `None` for a finding that names none (cross-layer findings and hand-built test
/// values both reach here). A finding with no file cannot be classified and takes the undemoted default —
/// `None` must never read as "matched nothing, therefore build surface".
fn file_of(f: &serde_json::Value) -> Option<&str> {
    f.get("file").and_then(|v| v.as_str())
}

/// True when the finding's file is a test path by the shared `${test-paths}` vocabulary. Kept separate
/// from [`deployment_role`] because the `testPaths` disclosure counts THIS predicate, unchanged, rather
/// than the role — the sentence that key has published since 2026-08-09 is about test paths, and a count
/// that quietly became "test paths minus the ones that are also build surface" would make that sentence
/// false without moving a byte of it.
pub(crate) fn is_test_path(f: &serde_json::Value) -> bool {
    file_of(f).is_some_and(|p| zzop_facade::test_path_re().is_match(p))
}

/// True when the finding's file is build/release surface — either the tree's own manifest named it from a
/// `scripts` command, or it carries one of the two ecosystem-fixed build path shapes.
pub(crate) fn is_build_surface(
    f: &serde_json::Value,
    build_script_paths: &std::collections::HashSet<&str>,
) -> bool {
    file_of(f)
        .is_some_and(|p| build_script_paths.contains(p) || zzop_facade::build_path_re().is_match(p))
}

/// The finding's deployment role — see the module doc for the three values and the precedence.
pub(crate) fn deployment_role(
    f: &serde_json::Value,
    build_script_paths: &std::collections::HashSet<&str>,
) -> u8 {
    if is_build_surface(f, build_script_paths) {
        ROLE_BUILD_SURFACE
    } else if is_test_path(f) {
        ROLE_TEST_PATH
    } else {
        ROLE_SHIPPED
    }
}

/// The `buildPaths` disclosure body — the count plus the sentence that makes the demotion visible. Built
/// here, beside the judgment it describes, so a future arm added to [`is_build_surface`] cannot ship with
/// a `meaning` that no longer names what it demotes.
///
/// Only the CALLER decides whether it rides the reply: additive-only, exactly like `truncated` and
/// `testPaths` — present when it has something to say, ABSENT (not `{"count": 0}`) otherwise. That
/// distinction is the whole point on a tree with no build surface at all: an always-present zero is a
/// field an agent must read and discard on every reply, while an absent key says "this axis found nothing
/// here" in no bytes at all.
pub(crate) fn build_paths_disclosure(count: usize) -> serde_json::Value {
    serde_json::json!({
        "count": count,
        "meaning": "findings whose file is build/release surface rather than shipped code — one this \
                    tree's own package.json `scripts` names (see the engine's `buildScriptPaths`), a \
                    file under `.github/` (CI workflow, composite action, repo template), or a \
                    `*.example` template. They are still real findings and every count includes them, \
                    but they sort after both production and test-path findings, whatever severity any \
                    of them carries, so a first screen leads with code that ships. Nothing is dropped \
                    and no severity changes.",
    })
}
