//! The envelope entry points: `analyzeEnvelope` and `validateEnvelopeOnly`
//! (`docs/NORMALIZED_AST.md`'s protocol receivers).

use serde::Serialize;

use zzop_core::NormalizedEnvelope;

use crate::config::{base_engine_config, fold_mounts};
use crate::output::SingleTreeOutputView;
use crate::request::{EnvelopeAnalyzeRequest, PacksDir};

/// `analyzeEnvelope(envelopeJson, configJson)` (`docs/NORMALIZED_AST.md`'s protocol receiver): validates
/// `envelopeJson` against the v1 Normalized AST contract (`zzop_core::validate_envelope` — a wrong
/// `format`/too-new `version`/empty or duplicate `path`/inverted `body_start`..`body_end` all fail here
/// with a structured, joined message, never a panic), deserializes `configJson` into an
/// `EnvelopeAnalyzeRequest`, and runs `zzop_engine::analyze_envelope`. Same JSON-string-in/JSON-string-out,
/// `AnalyzeOutputView`-serialized shape as `analyze_json`/`analyze_trees_json`.
///
/// ## Bundled-pack default (the one facade-level default in this crate)
/// The tree entry points (`analyze`/`analyzeTrees`) get their bundled-pack default from `zzop-config`'s
/// mapper (inline `packDefs`). The envelope path has no host config front-end on the Rust side at all,
/// so the same "zero-config = full analysis" default is applied HERE, once, for every host: the bundled
/// packs (`zzop_config::BUNDLED_PACK_SOURCES`) are seeded as inline `packDefs` BEFORE any
/// caller-supplied `packDefs`/`packsDir`, so the existing collision rules are untouched — a caller pack
/// with a bundled id wins the collision whole (later inline def wins; a directory pack always wins).
/// An explicit `"packsDir": null` opts out of every DSL pack, bundled included (the documented
/// opt-out, preserved by `EnvelopeAnalyzeRequest::packs_dir`'s absent-vs-null distinction).
/// Only `symbol-scan`/`io-scan` rules can fire in envelope mode (no source text — see
/// `zzop_engine::envelope`), so a bundled pack with neither contributes `packsLoaded` provenance
/// (`source: "inline"`) but no findings.
pub fn analyze_envelope_json(envelope_json: &str, config_json: &str) -> Result<String, String> {
    let envelope: NormalizedEnvelope =
        zzop_core::validate_envelope(envelope_json).map_err(|errors| {
            format!(
                "zzop-facade: invalid analyzeEnvelope() envelope JSON: {}",
                errors.join("; ")
            )
        })?;
    let req: EnvelopeAnalyzeRequest = serde_json::from_str(config_json)
        .map_err(|e| format!("zzop-facade: invalid analyzeEnvelope() config JSON: {e}"))?;

    let mut warnings = Vec::new();
    let packs_opt_out = matches!(req.packs_dir, Some(None)); // explicit `"packsDir": null`
    let packs_dirs = req
        .packs_dir
        .as_ref()
        .and_then(Option::as_ref)
        .map(PacksDir::as_dirs)
        .unwrap_or_default();
    let mut pack_defs = if packs_opt_out {
        Vec::new()
    } else {
        bundled_pack_defs(&mut warnings)
    };
    pack_defs.extend(req.pack_defs.iter().cloned());
    let mut config = base_engine_config(
        &req.source_id,
        &pack_defs,
        &packs_dirs,
        &req.disabled_rules,
        &req.severity_overrides,
        &req.suppressions,
        &req.global_excludes,
        &mut warnings,
    );
    // Deployment-topology mounts — the same `fold_mounts` fold `build_engine_config` applies for
    // tree-rooted requests (mounts[] first, `mountedAt` as the implicit `dir: ""` entry LAST), so the
    // engine's uniform envelope-mode mount apply (`analyze_envelope`'s `apply_config_mounts` call —
    // `docs/NORMALIZED_AST.md`'s Mode-A parity promise) is reachable over this wire path too.
    config.mounts = fold_mounts(&req.mounts, req.mounted_at.as_deref());
    let mut output = zzop_engine::analyze_envelope(&envelope, &config);
    warnings.append(&mut output.warnings);
    output.warnings = warnings;

    serde_json::to_string(&SingleTreeOutputView::of(&output))
        .map_err(|e| format!("zzop-facade: failed to serialize analyzeEnvelope() output: {e}"))
}

/// The bundled rule packs (`zzop_config::BUNDLED_PACK_SOURCES`, the workspace's one compile-time
/// embed of `rules/dsl/**/*.json`) parsed into inline `RulePackDef` seeds for
/// [`analyze_envelope_json`]'s bundled-pack default. Parsing goes through the loader's own verdict
/// path (`zzop_core::parse_dsl_pack`), and a pack that fails it — impossible for a committed bundled
/// pack unless the embed itself is broken — is skipped with a warning, never a failure (the same
/// "surface, don't crash" stance as a bad `packsDir` entry).
fn bundled_pack_defs(warnings: &mut Vec<String>) -> Vec<zzop_core::RulePackDef> {
    let mut defs = Vec::with_capacity(zzop_config::BUNDLED_PACK_SOURCES.len());
    for (rel_path, source) in zzop_config::BUNDLED_PACK_SOURCES {
        match zzop_core::parse_dsl_pack(source) {
            Ok(pack) => defs.push(pack),
            Err(err) => warnings.push(format!(
                "bundled pack \"{rel_path}\" failed to parse and was skipped: {err}."
            )),
        }
    }
    defs
}

/// A JSON-serializable `{valid, issues}` report — the shared output shape of
/// [`validate_envelope_only_json`] and [`crate::rule_pack::validate_rule_pack_json`].
#[derive(Serialize)]
pub(crate) struct ValidateReport {
    pub(crate) valid: bool,
    pub(crate) issues: Vec<String>,
}

/// `validateEnvelopeOnly(envelopeJson)`: runs `zzop_core::validate_envelope` alone — no `configJson`, no
/// pack loading, no `zzop_engine::analyze_envelope` — and reports the result as a JSON `{"valid": bool,
/// "issues": ["..."]}`. This is `analyze_envelope_json`'s validation half (see its use of
/// `zzop_core::validate_envelope` above) split out on its own so an external adapter author gets fast,
/// offline "is my envelope well-formed" feedback (`zzop-mcp validate-envelope <path>`) without needing a full
/// engine run or even a `configJson` at all.
///
/// Unlike every other `*_json` function in this crate, this one never fails: an unparseable or
/// semantically invalid envelope still produces an ordinary `{"valid": false, "issues": [...]}` report,
/// not an `Err` — a validity CHECK cannot itself be "wrong" the way a malformed request can, so there is
/// nothing here for `addon.rs`'s `catch` to turn into a JS `Error` except an actual panic.
pub fn validate_envelope_only_json(envelope_json: &str) -> String {
    let report = match zzop_core::validate_envelope(envelope_json) {
        Ok(_) => ValidateReport {
            valid: true,
            issues: Vec::new(),
        },
        Err(issues) => ValidateReport {
            valid: false,
            issues,
        },
    };

    serde_json::to_string(&report).unwrap_or_else(|e| {
        format!(
            r#"{{"valid":false,"issues":["zzop-facade: failed to serialize validate report: {e}"]}}"#
        )
    })
}
