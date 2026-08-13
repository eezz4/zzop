//! `[[test]]` target for the EXPORTED `examples/packs/code-hygiene.json` pack.
//!
//! Third and last increment of the `axis: opinion` export, and the widest: eight rules left THREE
//! packs at once (`browser` 1, `egress` 1, `reliability` 6), all three of which stay bundled and keep
//! shipping their defect rules. Nothing was deleted — see `examples/packs/README.md` for the rule this
//! repo follows on export, and `examples/packs/tests/sql_preferences.rs` for the increment that
//! established it.
//!
//! Wired from `rules/Cargo.toml` like every bundled pack; the only difference is the path it loads
//! (`../examples/packs` instead of `dsl`), because `CARGO_MANIFEST_DIR` is still `rules/`.
//!
//! ## What the eight share
//!
//! Each names a real trade rather than a defect. A blocking `confirm()` is the wrong control in an app
//! with a design system and the right one in a 200-line admin page. `process.exit()` in a function is a
//! crash in a library and the correct ending for a CLI. A committed `http://localhost:3000` is broken
//! in production and exactly right in a dev-only fixture. `console.log` in `api/` is unstructured
//! output — and unstructured output is what a small service's operator actually reads. Every one of
//! them gives a project that decided otherwise one finding per occurrence, forever.
//!
//! ## Two packs, on purpose — the localhost handoff
//!
//! One of the eight was a SIBLING'S REASON FOR SILENCE, the same class increment B found in
//! `sql/destructive-migration`. `egress/http-url-literal` stays bundled and excludes
//! localhost/private-IP literals outright; the bundled native `cross-layer/external-ip-literal` skips
//! loopback hosts and its message says whose turf that is. Both deferred to
//! `localhost-url-literal-committed`, which is now in THIS pack. A test asserting only one side of that
//! handoff cannot fail when the handoff breaks, so [`scan_both`] loads the bundled `egress` pack
//! alongside this one and asserts the routing — one rule fires while its cross-pack sibling stays
//! silent. `rules/dsl/egress/http_shapes.rs` keeps the mirror image of the same assertion.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_core::{load_dsl_packs, Finding, RulePackDef};
use zzop_engine::{analyze_tree, AnalyzeOutput, DispatchConfig, EngineConfig, DEFAULT_SIZE_CAP};

mod console_in_be;
mod console_in_loop;
mod egress_handoff;
mod env_nonnull_assert;
mod env_outside_config;
mod localhost_egress;
mod process_exit_and_race;
mod system_dialogs;
mod w2_languages;

pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new(prefix: &str) -> Self {
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

    pub fn path(&self) -> &Path {
        &self.0
    }

    pub fn write(&self, rel: &str, content: &str) {
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

/// The exported pack, loaded through `load_dsl_packs` so its `${NAME}` fragment refs resolve exactly as
/// they do at real load time — the moved rules reference the SHARED `test-paths`/`test-paths-stories`
/// vocabulary, and a raw `serde_json::from_str` would leave the literal `"${test-paths}"` in place,
/// which is not a valid regex and would silently no-op every affected rule. (This pack carries no
/// pack-local fragments: the two its source packs own — `browser`'s `html-sink-sanitized` and
/// `reliability`'s `test-paths-stories-scripts` — are referenced only by rules that STAYED, so copying
/// either here would have been dead data.) The `OnceLock` is the same one every pack test uses and for
/// the same reason: the disk load parses every pack in the directory and throws the rest away, and the
/// clone SHARES the compiled regex memo (`zzop_core::dsl::RegexCache`).
pub fn hygiene_pack() -> RulePackDef {
    static PACK: std::sync::OnceLock<RulePackDef> = std::sync::OnceLock::new();
    PACK.get_or_init(|| pack_from(Path::new("../examples/packs"), "code-hygiene"))
        .clone()
}

/// The BUNDLED `egress` pack one of these rules split off from — needed only by [`scan_both`]'s handoff
/// test, never to judge one of this pack's own rules.
pub fn bundled_egress_pack() -> RulePackDef {
    static PACK: std::sync::OnceLock<RulePackDef> = std::sync::OnceLock::new();
    PACK.get_or_init(|| pack_from(Path::new("dsl/egress"), "egress"))
        .clone()
}

fn pack_from(rel: &Path, id: &str) -> RulePackDef {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    let result = load_dsl_packs(&dir);
    assert!(
        result.errors.is_empty(),
        "pack load errors under {}: {:?}",
        dir.display(),
        result.errors
    );
    result
        .packs
        .into_iter()
        .map(|(_, pack)| pack)
        .find(|p| p.id == id)
        .unwrap_or_else(|| panic!("no pack `{id}` under {}", dir.display()))
}

fn config(packs: Vec<RulePackDef>) -> EngineConfig {
    EngineConfig {
        source_id: "code-hygiene-fixture".to_string(),
        dispatch: DispatchConfig::default(),
        size_cap: DEFAULT_SIZE_CAP,
        rule_config: Default::default(),
        packs,
        ..EngineConfig::default()
    }
}

/// Scan with the exported pack alone — the default, and what a user who retrieves only this pack gets.
pub fn scan(dir: &TempDir) -> AnalyzeOutput {
    analyze_tree(dir.path(), &config(vec![hygiene_pack()]))
}

/// Scan with BOTH this pack and the bundled `egress` pack. Only for the handoff assertions described in
/// this file's header: a test that must show one rule firing WHILE its cross-pack sibling stays silent
/// cannot get that from a single-pack run, where the sibling's silence is vacuous.
pub fn scan_both(dir: &TempDir) -> AnalyzeOutput {
    analyze_tree(
        dir.path(),
        &config(vec![hygiene_pack(), bundled_egress_pack()]),
    )
}

/// `scan` with one Mode-B adapter overlay attached — the channel a `zzop.config.jsonc` author reaches
/// through the `overlays` key, and the only way a declaration-gated rule (`env-outside-config`) can be
/// exercised in its ENABLED state. Moved here with that rule from `rules/dsl/reliability/reliability.rs`,
/// where it was the only consumer left once the rule was exported.
pub fn scan_with(dir: &TempDir, overlay: zzop_core::NormalizedEnvelope) -> AnalyzeOutput {
    let mut cfg = config(vec![hygiene_pack()]);
    cfg.adapter_overlays = vec![overlay];
    analyze_tree(dir.path(), &cfg)
}

/// An attributes-only overlay declaring each entry of `prefixes` an `env-config-module`. A path that
/// names a directory is a covering `pathScope`; the same call is used for an exact file path, which
/// resolves as the more specific `pathScope` of that exact string — [`deny_env_config_for_file`] is what
/// adds a genuine exact-`file` target.
///
/// The envelope carries a single synthetic file entry, because `AttributeStore::from_parts` flattens
/// `files[].attributes` tree-wide and never cares which file emitted them — the same shape
/// `examples/adapters/auth-overlay-adapter` emits.
pub fn env_config_overlay(prefixes: &[&str]) -> zzop_core::NormalizedEnvelope {
    overlay_with_attributes(
        prefixes
            .iter()
            .map(|p| zzop_core::Attribute {
                target: zzop_core::EntityRef::PathScope {
                    prefix: (*p).to_string(),
                },
                key: "env-config-module".to_string(),
                value: serde_json::Value::Bool(true),
            })
            .collect(),
    )
}

/// Appends an exact-`file` `env-config-module: false` to `overlay` — the carve-out spelling: an exact
/// target beats every covering scope, so this un-declares one file inside a declared directory.
pub fn deny_env_config_for_file(overlay: &mut zzop_core::NormalizedEnvelope, path: &str) {
    overlay.files[0].attributes.push(zzop_core::Attribute {
        target: zzop_core::EntityRef::File {
            path: path.to_string(),
        },
        key: "env-config-module".to_string(),
        value: serde_json::Value::Bool(false),
    });
}

fn overlay_with_attributes(attributes: Vec<zzop_core::Attribute>) -> zzop_core::NormalizedEnvelope {
    zzop_core::NormalizedEnvelope {
        format: zzop_core::NORMALIZED_AST_FORMAT.to_string(),
        version: zzop_core::NORMALIZED_AST_CONTRACT_VERSION.to_string(),
        parser: "code-hygiene-fixture-declarations/1".to_string(),
        source: String::new(),
        files: vec![zzop_core::FileProjection {
            path: "zzop-attributes.json".to_string(),
            loc: 1,
            attributes,
            ..Default::default()
        }],
    }
}

/// Every `AnalyzeOutput::warnings` entry mentioning `needle` — the disclosure channel a silenced rule
/// reports through.
pub fn warnings_matching<'a>(out: &'a AnalyzeOutput, needle: &str) -> Vec<&'a String> {
    out.warnings.iter().filter(|w| w.contains(needle)).collect()
}

/// Findings of one of THIS pack's rules, by bare id.
pub fn hits<'a>(out: &'a AnalyzeOutput, rule: &str) -> Vec<&'a Finding> {
    with_prefix(out, "code-hygiene", rule)
}

/// Findings of a rule that stayed in the bundled `egress` pack — the other half of a handoff assertion.
pub fn egress_hits<'a>(out: &'a AnalyzeOutput, rule: &str) -> Vec<&'a Finding> {
    with_prefix(out, "egress", rule)
}

fn with_prefix<'a>(out: &'a AnalyzeOutput, pack: &str, rule: &str) -> Vec<&'a Finding> {
    out.findings
        .iter()
        .filter(|f| f.rule_id == format!("{pack}/{rule}"))
        .collect()
}
