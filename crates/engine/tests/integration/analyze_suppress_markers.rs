//! THE SUPPRESSION CENSUS — `zzop-<rule>-ok` markers, disclosed per tree
//! (`analyze::diagnostics::suppressed_findings_warning`, fed by `pipeline::FileArtifact::
//! suppress_markers`).
//!
//! The gap it closes is the last silence in this product with no trace in the output. Suppression
//! itself works precisely — the marker silences exactly its own rule on exactly its own line — and that
//! precision is why the absence was invisible: a suppressed finding is simply not in `findings`, so
//! `findings.total` reads one lower and no other number moves. Measured on the shape below, a hardcoded
//! production credential quieted by one comment left the whole reply carrying no key matching
//! `/suppress/i`.
//!
//! Two properties are load-bearing here and neither is obvious from the warning's text:
//! - it survives a WARM CACHE, because the census is a function of the file's bytes rather than of rule
//!   evaluation (a full cache hit runs no rule at all, so an evaluation-derived count would be right
//!   cold and zero on every rerun — the failure mode of the thing being disclosed);
//! - it counts markers PRESENT, so a stale marker that silences nothing is listed, and the message says
//!   so rather than letting the count be read as "N real findings were hidden".

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_engine::{analyze_tree, AnalyzeOutput, EngineConfig};

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

const HEAD: &str = "suppression marker";

fn census_line(out: &AnalyzeOutput) -> Option<&String> {
    out.warnings.iter().find(|w| w.contains(HEAD))
}

/// The motivating shape: a real credential, silenced by a comment, in a tree that otherwise looks
/// clean. The census must name the FILE, the LINE and the MARKER — the marker most of all, because the
/// marker is the rule, and "was anything security-shaped quieted here" has to be answerable without
/// opening the file.
#[test]
fn a_silenced_credential_is_named_by_file_line_and_marker() {
    let dir = TempDir::new("zzop-suppress-census");
    dir.write(
        "src/mailer.ts",
        "export const SMTP_PASSWORD = \"hunter2-prod-mailer-pass\"; // zzop-hardcoded-secret-ok\n",
    );

    let out = analyze_tree(dir.path(), &EngineConfig::default());
    let line = census_line(&out)
        .unwrap_or_else(|| panic!("the marker must be disclosed: {:?}", out.warnings));
    assert!(
        line.contains("src/mailer.ts:1") && line.contains("zzop-hardcoded-secret-ok"),
        "the census must name the site and the marker token: {line}"
    );
    // The overclaim guard, in the message's own words: this counts markers, not suppressions.
    assert!(
        line.contains("counts markers PRESENT, not findings suppressed"),
        "the census must refuse the stronger reading of its own number: {line}"
    );
}

/// Silence is the default. Without this, a warning that fired on every run would pass the test above
/// while telling a reader nothing.
#[test]
fn a_tree_with_no_markers_says_nothing() {
    let dir = TempDir::new("zzop-suppress-census-none");
    dir.write("src/a.ts", "export const a = 1; // an ordinary comment\n");
    dir.write("src/b.ts", "export const b = 2; // TODO: not a marker\n");

    let out = analyze_tree(dir.path(), &EngineConfig::default());
    assert!(
        census_line(&out).is_none(),
        "no markers, no line: {:?}",
        out.warnings
    );
}

/// A non-zzop `-ok` comment is a different mechanism and must not be counted — the census is derived
/// from the token shape `RuleDef::suppress_marker_for_id` actually mints, so a rule-agnostic `-ok`
/// convention in someone's codebase cannot inflate it.
#[test]
fn an_unrelated_ok_comment_is_not_a_zzop_marker() {
    let dir = TempDir::new("zzop-suppress-census-foreign");
    dir.write("src/a.ts", "export const a = 1; // idempotent-ok\n");
    dir.write("src/b.ts", "export const b = 2; // looks-ok\n");

    let out = analyze_tree(dir.path(), &EngineConfig::default());
    assert!(
        census_line(&out).is_none(),
        "only `zzop-...-ok` is this mechanism: {:?}",
        out.warnings
    );
}

/// THE CACHE PROPERTY, and the reason the census is computed where it is. The second run over an
/// unchanged tree serves files from the cache and runs no rule for them — an evaluation-derived count
/// would go to zero here, silently, which is precisely the failure this disclosure exists to prevent.
#[test]
fn the_census_survives_a_warm_cache() {
    let dir = TempDir::new("zzop-suppress-census-warm");
    dir.write(
        "src/mailer.ts",
        "export const SMTP_PASSWORD = \"hunter2-prod-mailer-pass\"; // zzop-hardcoded-secret-ok\n",
    );
    let config = EngineConfig {
        cache_dir: Some(dir.path().join(".zzop/cache")),
        ..EngineConfig::default()
    };

    let cold = analyze_tree(dir.path(), &config);
    let cold_line = census_line(&cold)
        .unwrap_or_else(|| panic!("cold run must disclose: {:?}", cold.warnings))
        .clone();
    assert_eq!(
        cold.cache.as_ref().map(|c| c.hits),
        Some(0),
        "the first run must be cold, or this test proves nothing"
    );

    let warm = analyze_tree(dir.path(), &config);
    assert!(
        warm.cache.as_ref().is_some_and(|c| c.hits > 0),
        "the second run must actually hit the cache, or this test proves nothing: {:?}",
        warm.cache
    );
    assert_eq!(
        census_line(&warm),
        Some(&cold_line),
        "a warm run must disclose the identical census — the count is taken from the file's bytes, \
         never from rule evaluation the cache skips: {:?}",
        warm.warnings
    );
}

/// Multiple markers across files: the count and the distinct-rule tally are both over the whole tree,
/// and the sample is capped rather than dumping every site.
#[test]
fn many_markers_are_counted_whole_and_sampled() {
    let dir = TempDir::new("zzop-suppress-census-many");
    for i in 0..5 {
        dir.write(
            &format!("src/m{i}.ts"),
            "export const API_KEY = \"sk-aaaaaaaaaaaaaaaaaaaa\"; // zzop-hardcoded-secret-ok\n",
        );
    }
    dir.write(
        "src/z.py",
        "PASSWORD = \"aaaaaaaaaaaaaaaa\"  # zzop-high-entropy-secret-ok\n",
    );

    let out = analyze_tree(dir.path(), &EngineConfig::default());
    let line = census_line(&out)
        .unwrap_or_else(|| panic!("markers must be disclosed: {:?}", out.warnings));
    assert!(
        line.starts_with("6 suppression markers in this tree, spanning 2 rule id(s)"),
        "count and distinct-rule tally are over the whole tree: {line}"
    );
    assert!(
        line.contains("+3 more"),
        "the site list is sampled, and says how many it dropped: {line}"
    );
    // Note what proves the `#`-leader file was seen: the RULE TALLY above, not the sample. The sample
    // is capped at three sites and the Python file sorts past it — asserting on the sampled text here
    // would be asserting on the cap.
}

/// The `#` comment leader, on its own so it lands in the sample. Suppression itself honors `#` for the
/// hash-comment languages, and a census that only read `//` would under-report every one of them while
/// suppression kept working there — a disclosure narrower than the thing it discloses.
#[test]
fn the_census_reads_the_files_own_comment_leaders() {
    let dir = TempDir::new("zzop-suppress-census-hash");
    dir.write(
        "app.py",
        "PASSWORD = \"aaaaaaaaaaaaaaaa\"  # zzop-high-entropy-secret-ok\n",
    );

    let out = analyze_tree(dir.path(), &EngineConfig::default());
    let line = census_line(&out)
        .unwrap_or_else(|| panic!("a `#` marker must be disclosed: {:?}", out.warnings));
    assert!(
        line.contains("app.py:1") && line.contains("zzop-high-entropy-secret-ok"),
        "the census must read `#` for a Python file, as suppression does: {line}"
    );
}
