//! DSL rule-pack loader — reads every `rules/dsl/*.json` under a directory into `RulePackDef`, plus the
//! per-pack file-path pre-filter that decides whether a pack has any rule that could ever fire against a
//! given file.
//!
//! Two directory shapes are supported, and may be mixed in the same directory: flat (`<dir>/<id>.json`,
//! what an external/user `packsDir` typically uses) and depth-1 nested (`<dir>/<name>/<id>.json`, the
//! "co-located pack folder" layout this repo's own first-party packs ship in — see `rules/README.md`).
//! Both are valid; nesting is never required.
//!
//! ## Where "appliesTo" lives (design call)
//! Gating a whole pack on the TARGET environment (fe/be/ext-chrome/...) belongs on `RulePackDef` (dsl.rs)'s
//! `framework` field ("any" | "react" | "prisma" | ...) — and that's exactly what `RuleMeta::applies_to`
//! (registry.rs) already gates on for every rule layer uniformly. `RulePackDef` does NOT carry a file-path
//! / language-extension
//! field at the pack level, though: file-path gating lives PER RULE, inside its matcher
//! (`Matcher::{LineScan,MethodScan}.file_pattern`) — a single pack can mix, say, a `.java` rule and a `.jsp`
//! rule. So `applies_to` below is a narrower, additional pre-filter: "does at least one rule in this pack
//! even look at files shaped like `file_path`" — useful for a caller that wants to skip considering a pack
//! entirely for a tree of files none of its rules could ever match. It is NOT a substitute for the
//! framework/target gating, which stays on `RuleMeta::applies_to`.

use std::fs;
use std::path::{Path, PathBuf};

use crate::dsl::{Matcher, RulePackDef};

/// The highest `RulePackDef::schema_version` this engine build understands (see `docs/rules/dsl-reference.md`'s
/// "Schema version policy"). A pack declaring a higher version depends on core IR/matcher shapes
/// this build predates — loading it anyway would silently misinterpret fields this engine has never seen
/// (or, worse, seen with a different meaning), so `load_dsl_packs` rejects it outright as a per-file
/// `PackLoadError` instead. Bump this constant only when a genuinely new, incompatible-with-older-builds
/// DSL schema revision ships; ordinary additive changes (new optional matcher fields with
/// `#[serde(default)]`) do not need a bump — older packs already deserialize fine against them, and this
/// constant only gates the other direction (a pack newer than the engine).
pub const SUPPORTED_DSL_SCHEMA_VERSION: u32 = 1;

/// One `*.json` file under the pack directory that failed to read, deserialize, or pass the schema-version
/// gate — a per-file error, not a panic: a single malformed/too-new pack must not take down every other
/// pack in the directory.
#[derive(Debug, Clone)]
pub struct PackLoadError {
    pub path: PathBuf,
    pub message: String,
}

/// Result of scanning a directory of DSL rule packs. `packs` is sorted by full path for determinism
/// (registration/evaluation order must not depend on OS directory-iteration order, and must stay
/// deterministic across a mix of flat and depth-1-nested pack files); `errors` holds one entry per file
/// that failed to read or parse.
#[derive(Debug, Default)]
pub struct LoadResult {
    pub packs: Vec<(PathBuf, RulePackDef)>,
    pub errors: Vec<PackLoadError>,
}

/// The single "pack JSON text -> loadable `RulePackDef`" judgment: serde deserialization (missing
/// field / wrong type errors come back verbatim as serde's own message), the schema-version gate above,
/// then `RulePackDef::expand_fragments` — an unknown or malformed `${NAME}` fragment reference fails the
/// load exactly like a bad JSON body does, never a silent passthrough. This is the exact per-file step
/// [`load_dsl_packs`] applies to every `rules/dsl/*.json`, extracted so a pre-load validator
/// (`zzop_facade::validate_rule_pack_json`, the `validate_rule_pack` MCP tool / `zzop-mcp validate-rule-pack` CLI)
/// surfaces the SAME verdicts the loader would produce at load time — one path, no forked logic. The
/// returned pack's `fragments` map is always empty (see `expand_fragments`'s doc) — every pattern-bearing
/// field already carries its resolved regex text, so nothing downstream (hashing, `RegexSet` prefilter,
/// eval) needs to know fragments ever existed.
pub fn parse_dsl_pack(text: &str) -> Result<RulePackDef, String> {
    // A JSON ARRAY root is a special case — see `zzop_core::normalized::validate_envelope`'s identical
    // guard for the full rationale. `RulePackDef`'s derived `Deserialize` accepts a sequence as well as
    // a map, so a top-level array like `[1,2,3]` deserializes positionally against the struct's
    // declared fields instead of being rejected as "not an object": `1` lands on the first field
    // (`id: String`) and fails with a field-level "invalid type: integer `1`, expected a string" that
    // reads like one field is wrong, not "this isn't a rule pack at all". Caught here, before the
    // struct deserialize, with the honest diagnosis.
    if matches!(
        serde_json::from_str::<serde_json::Value>(text),
        Ok(serde_json::Value::Array(_))
    ) {
        return Err("expected a JSON object rule pack, got an array".to_string());
    }
    match serde_json::from_str::<RulePackDef>(text) {
        Ok(mut pack) => check_dsl_schema_version(&pack)
            .and_then(|()| pack.expand_fragments().map_err(|e| e.to_string()))
            .map(|()| pack),
        Err(err) => Err(err.to_string()),
    }
}

/// The schema-version gate on its own, for a pack that is ALREADY a deserialized `RulePackDef`
/// (an inline `packDefs` request entry never passes through [`parse_dsl_pack`]'s text path, but must
/// face the exact same verdict with the exact same wording — one wording, no fork). This is the one
/// place the "too new" message is composed; `parse_dsl_pack` delegates here.
pub fn check_dsl_schema_version(pack: &RulePackDef) -> Result<(), String> {
    if pack.schema_version > SUPPORTED_DSL_SCHEMA_VERSION {
        return Err(format!(
            "pack requires newer DSL schema (schema_version {}, this engine supports up to {})",
            pack.schema_version, SUPPORTED_DSL_SCHEMA_VERSION
        ));
    }
    Ok(())
}

/// Every regex-typed field in `pack` that fails to compile, as one issue string each (deterministic:
/// rule order, then field order within the matcher). This surfaces, at validation time, the exact
/// judgment the DSL interpreter applies at eval time — `regex::Regex::new(p)` failing — where the
/// interpreter's contract is to silently no-op the affected rule (see `dsl::line_scan`/`method_scan`/
/// `ir_scan` and [`applies_to`] below) rather than panic. A pack with such an issue still LOADS; it
/// just carries a rule that can never fire, which is exactly what a pack author wants told before
/// shipping it.
pub fn pack_regex_issues(pack: &RulePackDef) -> Vec<String> {
    let mut issues = Vec::new();
    for rule in &pack.rules {
        let mut check = |field: &str, pattern: &str| {
            if let Err(err) = regex::Regex::new(pattern) {
                issues.push(format!(
                    "rule \"{}\": `{field}` is not a valid regex (the rule would silently never fire): {err}",
                    rule.id
                ));
            }
        };
        match &rule.matcher {
            Matcher::LineScan(m) => {
                check("file_pattern", &m.file_pattern);
                if let Some(p) = &m.require_file {
                    check("require_file", p);
                }
                for p in &m.require_file_all {
                    check("require_file_all", p);
                }
                for p in &m.require_file_absent {
                    check("require_file_absent", p);
                }
                if let Some(p) = &m.line_pattern {
                    check("line_pattern", p);
                }
                for lp in m.any.iter().flatten() {
                    check("any[].pattern", &lp.pattern);
                }
                if let Some(p) = &m.exclude_pattern {
                    check("exclude_pattern", p);
                }
                if let Some(p) = &m.file_exclude_pattern {
                    check("file_exclude_pattern", p);
                }
            }
            Matcher::MethodScan(m) => {
                check("file_pattern", &m.file_pattern);
                if let Some(p) = &m.require_file {
                    check("require_file", p);
                }
                for p in &m.require_file_all {
                    check("require_file_all", p);
                }
                for p in &m.require_file_absent {
                    check("require_file_absent", p);
                }
                for lp in &m.patterns {
                    check("patterns[].pattern", &lp.pattern);
                }
                for lp in &m.absent {
                    check("absent[].pattern", &lp.pattern);
                }
                if let Some(p) = &m.file_exclude_pattern {
                    check("file_exclude_pattern", p);
                }
            }
            Matcher::SymbolScan(m) => {
                check("file_pattern", &m.file_pattern);
                if let Some(p) = &m.name_pattern {
                    check("name_pattern", p);
                }
            }
            Matcher::IoScan(m) => {
                check("file_pattern", &m.file_pattern);
                if let Some(p) = &m.key_pattern {
                    check("key_pattern", p);
                }
            }
        }
    }
    issues
}

/// Reads every `*.json` file directly under `dir`, PLUS every `*.json` file one level down inside a
/// subdirectory of `dir` (`<dir>/<name>/*.json`) — see the module doc for the two supported shapes — and
/// deserializes each into a `RulePackDef`. Only one level of subdirectory is scanned (a
/// sub-subdirectory's `*.json` is not found): deliberately shallow, matching "one folder per pack" rather
/// than an arbitrary recursive tree. Directory-read failure (missing/unreadable dir) is reported as a
/// single error entry (path = `dir`) rather than a panic, same "surface, don't crash" contract as a
/// malformed pack file.
pub fn load_dsl_packs(dir: &Path) -> LoadResult {
    let mut result = LoadResult::default();
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(err) => {
            result.errors.push(PackLoadError {
                path: dir.to_path_buf(),
                message: err.to_string(),
            });
            return result;
        }
    };

    let mut paths: Vec<PathBuf> = Vec::new();
    let mut subdirs: Vec<PathBuf> = Vec::new();
    for entry in entries.filter_map(Result::ok) {
        let p = entry.path();
        if p.is_file() && p.extension().and_then(|e| e.to_str()) == Some("json") {
            paths.push(p);
        } else if p.is_dir() {
            subdirs.push(p);
        }
    }
    for sub in subdirs {
        if let Ok(sub_entries) = fs::read_dir(&sub) {
            for entry in sub_entries.filter_map(Result::ok) {
                let p = entry.path();
                if p.is_file() && p.extension().and_then(|e| e.to_str()) == Some("json") {
                    paths.push(p);
                }
            }
        }
    }
    // Sort by full path (not just file name) so load order is deterministic across BOTH the flat and
    // nested shapes, regardless of directory-listing order.
    paths.sort();

    for path in paths {
        // Per-file verdicts come from `parse_dsl_pack` — the same judgment path the pre-load
        // validator (`validate_rule_pack`) surfaces, so the two can never disagree.
        match fs::read_to_string(&path) {
            Ok(text) => match parse_dsl_pack(&text) {
                Ok(pack) => result.packs.push((path, pack)),
                Err(message) => result.errors.push(PackLoadError { path, message }),
            },
            Err(err) => result.errors.push(PackLoadError {
                path,
                message: err.to_string(),
            }),
        }
    }
    result
}

/// True if at least one rule in `pack` has a matcher whose `file_pattern` matches `file_path` — see the
/// module doc for why this is a per-rule pre-filter, not a whole-pack `appliesTo`. A rule whose
/// `file_pattern` fails to compile as a regex is treated as non-matching (mirrors `eval_pack`, which
/// already no-ops a rule with an invalid pattern rather than panicking).
pub fn applies_to(pack: &RulePackDef, file_path: &str) -> bool {
    pack.rules.iter().any(|rule| {
        let pattern = match &rule.matcher {
            Matcher::LineScan(m) => Some(&m.file_pattern),
            Matcher::MethodScan(m) => Some(&m.file_pattern),
            // Non-exhaustive on purpose: a future matcher kind without a `file_pattern` (or one this
            // module doesn't know about yet) is conservatively treated as "could match" rather than
            // silently excluding the pack — a false "applies" only costs an extra (skippable) pack
            // consideration, whereas a false "doesn't apply" would hide real findings.
            #[allow(unreachable_patterns)]
            _ => None,
        };
        match pattern {
            Some(p) => regex::Regex::new(p)
                .map(|re| re.is_match(file_path))
                .unwrap_or(false),
            None => true,
        }
    })
}

#[cfg(test)]
mod tests;
