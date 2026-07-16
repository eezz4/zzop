//! Contract 11's shared vocabulary + extraction/validation helpers — see `reference_validation.rs` for the
//! contract's own rationale header and the two real-tree tests these helpers serve.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// `packages/cli/lib/config-surface.json`'s path, resolved relative to this crate's own manifest dir (same
/// "sibling package, read across the tree, never hand-copied" pattern as `catalog_path`/`dsl_dir`/
/// `native_dir` above).
fn config_surface_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/cli/lib/config-surface.json")
}

/// Mirrors `config-surface.json`'s `configKeys` object.
#[derive(serde::Deserialize)]
pub(crate) struct ConfigKeysSurface {
    pub(crate) top: Vec<String>,
    pub(crate) packs: Vec<String>,
    pub(crate) git: Vec<String>,
    pub(crate) report: Vec<String>,
    pub(crate) tree: Vec<String>,
    #[serde(rename = "ruleObject")]
    pub(crate) rule_object: Vec<String>,
}

/// Mirrors `config-surface.json`'s top-level shape. `#[serde(rename_all = "camelCase")]` maps this
/// struct's snake_case field names to the file's camelCase keys; the file's own `_docs` field is simply
/// ignored (serde drops unrecognized fields by default — no `deny_unknown_fields` here on purpose, the
/// same "an older/newer consumer degrades to ignored" contract `crates/facade/src/lib.rs`'s own request
/// types document).
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConfigSurface {
    pub(crate) config_keys: ConfigKeysSurface,
    pub(crate) config_paths: Vec<String>,
    pub(crate) cli_flags: Vec<String>,
    pub(crate) embedder_fields: Vec<String>,
    /// One-line shape string per embedder field (the blind-agent breadcrumb section). `default`
    /// so an older file (or a consumer that predates the section) degrades to empty — the mirror
    /// test below is what enforces presence on THIS repo's own file.
    #[serde(default)]
    pub(crate) embedder_field_shapes: std::collections::BTreeMap<String, String>,
    pub(crate) external_tool_flags: Vec<String>,
    pub(crate) allowlisted_tokens: Vec<String>,
}

/// Loads and parses `config-surface.json`, failing loudly (not silently skipping) on a missing/malformed
/// file — same "a load error would otherwise hide real data from every test below" reasoning as
/// `load_all_packs`'s doc above.
pub(crate) fn load_config_surface() -> ConfigSurface {
    let path = config_surface_path();
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("failed to parse {}: {e}", path.display()))
}

/// Whether `line` is a comment line — a Rust `//`-prefixed line, or a JS `//`/`/*`/`*`-prefixed line
/// (covers a JS line comment, a block-comment opener, and every continuation line of a `/* ... */` or
/// JSDoc `/** ... */` block in this codebase's own style, which always starts a continuation line with a
/// leading `*`). One check serves both scanned languages: a Rust doc comment (`///`/`//!`) already starts
/// with `//`, so the same predicate correctly treats it as a comment too. Pragmatic line-level check (same
/// "keep it pragmatic" spirit as this file's other proxies, e.g. `native_rule_files_that_build_findings_...`
/// above): it does not track true multi-line block-comment START/end state — a `/* ... */` block whose
/// continuation lines do NOT start with `*` would not be fully skipped — but every block comment in this
/// codebase's actual style does, so the gap has never mattered in practice. Applied identically for both
/// contract-11 checks below: a message reaching a REAL reader is a string literal sitting on a CODE line,
/// never inside a doc comment describing the convention.
fn is_comment_line(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("//") || t.starts_with("/*") || t.starts_with('*')
}

/// CHECK A's own regex: a `--`-prefixed, all-lowercase, hyphen/digit-friendly flag token — deliberately
/// matches the exact shape a CLI/tool flag is spelled in prose (`--since`, `--depth`, `--unshallow`), not a
/// general "starts with two dashes" scan (an em dash-adjacent `--` in prose, or a `--` inside a code
/// comment, is excluded by the comment-line skip above, not by this regex).
fn flag_reference_regex() -> regex::Regex {
    regex::Regex::new(r"--[a-z][a-z0-9-]{1,}").expect("static regex")
}

/// CHECK A extraction: every `--flag`-shaped token appearing on a non-comment line of `text`, in order.
/// Pure — no vocabulary lookup here, see `unknown_flag_references` for the validation step. Deliberately
/// line-level (not a single whole-file regex pass): comment-skipping is naturally a per-line decision (see
/// `is_comment_line`'s doc), and every flag reference this contract cares about is short enough to never
/// span a line break in practice.
pub(crate) fn extract_flag_references(text: &str) -> Vec<String> {
    let re = flag_reference_regex();
    let mut out = Vec::new();
    for line in text.lines() {
        if is_comment_line(line) {
            continue;
        }
        out.extend(re.find_iter(line).map(|m| m.as_str().to_string()));
    }
    out
}

/// CHECK A validation: which of `flags` names neither a real CLI flag nor a real external-tool flag —
/// i.e. which ones `config-surface.json` does not vouch for. Returns them in the order found (may contain
/// duplicates — the real-tree test below reports every occurrence, not just distinct offenders, so a
/// reader can see how many places need fixing).
pub(crate) fn unknown_flag_references(flags: &[String], vocab: &ConfigSurface) -> Vec<String> {
    let allowed: BTreeSet<&str> = vocab
        .cli_flags
        .iter()
        .map(String::as_str)
        .chain(vocab.external_tool_flags.iter().map(String::as_str))
        .collect();
    flags
        .iter()
        .filter(|f| !allowed.contains(f.as_str()))
        .cloned()
        .collect()
}

/// CHECK B's shape gate: a backtick-quoted token counts as "config-key-shaped" only when it looks like a
/// bare identifier or a dotted/bracketed path (`git.since`, `trees[].root`, `disabled_rules`) — NOT a JSON
/// snippet like `` rules: { "circular": "off" } `` (spaces/colons/braces/quotes all fail this shape), which
/// every native rule's own disable-hint legitimately embeds right next to the word "config". A token this
/// gate rejects is simply not checked at all (neither accepted nor an offender) — see this contract's
/// module-doc entry for why that is the intentionally narrow scope, not a gap.
fn config_key_shape_regex() -> regex::Regex {
    regex::Regex::new(r"^[A-Za-z_][A-Za-z0-9_]*([.\[\]]+[A-Za-z0-9_\[\]]*)*$")
        .expect("static regex")
}

/// CHECK B extraction: every backtick-quoted token sitting on a non-comment line of `text` within 120
/// bytes of a whole-word (case-insensitive) "config" occurrence — also required to itself be on a
/// non-comment line, so a rustdoc comment that happens to backtick-reference an ordinary Rust identifier
/// near its own prose use of "config" (extremely common in this codebase's `///`/`//!` docs — e.g.
/// `` `EngineConfig` ``, `` `ScoresConfig` ``) is never even considered. Both the "config" occurrences and
/// the backtick tokens are located in a COMMENT-BLANKED copy of `text` (`is_comment_line`-flagged lines
/// replaced with spaces of the same length) rather than filtering post-hoc, so byte offsets — and therefore
/// the ±120 distance itself — stay computed on a single consistent coordinate space that never lets a
/// "config" mention on one line anchor a match to a backtick token on an unrelated comment line one line
/// below/above it.
///
/// Word-boundary "config" matching (`\bconfig\b`, not a bare substring scan) is deliberate: this codebase's
/// source is full of identifiers that merely CONTAIN "config" (`EngineConfig`, `ScoresConfig`,
/// `RuleConfig`, `config.rs` filenames) without naming the word "config" on its own — a substring scan
/// production-tested against the real tree here turned up 170+ incidental hits from exactly that class
/// before switching to `\bconfig\b` cut it to the single-digit count this contract's allowlist actually
/// documents.
pub(crate) fn extract_config_context_tokens(text: &str) -> Vec<String> {
    let masked: String = text
        .lines()
        .map(|line| {
            if is_comment_line(line) {
                " ".repeat(line.len())
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    let config_re = regex::Regex::new(r"(?i)\bconfig\b").expect("static regex");
    let backtick_re = regex::Regex::new(r"`([^`]*)`").expect("static regex");

    let config_positions: Vec<usize> = config_re.find_iter(&masked).map(|m| m.start()).collect();
    if config_positions.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::new();
    for caps in backtick_re.captures_iter(&masked) {
        let whole = caps.get(0).expect("group 0 always matches");
        let near = config_positions
            .iter()
            .any(|&cpos| cpos.abs_diff(whole.start()) <= 120 || cpos.abs_diff(whole.end()) <= 120);
        if near {
            out.push(caps[1].to_string());
        }
    }
    out
}

/// CHECK B validation: which of `tokens` (as extracted by `extract_config_context_tokens`) names neither a
/// real config path/key nor an allowlisted/embedder token. A token that is not config-key-shaped at all
/// (`config_key_shape_regex` rejects it — e.g. a JSON snippet) is silently skipped, not reported: this
/// contract only judges tokens shaped like a knob a reader could actually go try.
///
/// Per-shape rule (mirrors the task's own two-branch contract):
/// - **Dotted/bracketed** (contains `.` or `[`): valid when the WHOLE token is in `configPaths` or
///   `allowlistedTokens`, OR its first segment (split on `.`/`[`) is in `embedderFields` — an embedder field
///   can legitimately be written with its own dotted continuation in a message (none do today, but the
///   shape is allowed the same way a bare embedder field name is).
/// - **Single word**: valid when it is a top-level config key, a nested key name from ANY of
///   `packs`/`git`/`report`/`tree`/`ruleObject` (the nested scopes are flattened into one name set here —
///   a message says `` `dir` `` or `` `since` `` without also repeating its parent, so there is no unambiguous
///   way to check a nested key against only its OWN parent's scope from the token text alone), an embedder
///   field, or an allowlisted token.
pub(crate) fn unknown_config_context_tokens(
    tokens: &[String],
    vocab: &ConfigSurface,
) -> Vec<String> {
    let shape = config_key_shape_regex();

    let top: BTreeSet<&str> = vocab.config_keys.top.iter().map(String::as_str).collect();
    let mut nested: BTreeSet<&str> = BTreeSet::new();
    for scope in [
        &vocab.config_keys.packs,
        &vocab.config_keys.git,
        &vocab.config_keys.report,
        &vocab.config_keys.tree,
        &vocab.config_keys.rule_object,
    ] {
        nested.extend(scope.iter().map(String::as_str));
    }
    let paths: BTreeSet<&str> = vocab.config_paths.iter().map(String::as_str).collect();
    let embedder: BTreeSet<&str> = vocab.embedder_fields.iter().map(String::as_str).collect();
    let allow: BTreeSet<&str> = vocab
        .allowlisted_tokens
        .iter()
        .map(String::as_str)
        .collect();

    tokens
        .iter()
        .filter(|t| {
            if !shape.is_match(t) {
                return false; // not config-key-shaped at all — out of scope, not an offense.
            }
            let is_dotted = t.contains('.') || t.contains('[');
            if is_dotted {
                if paths.contains(t.as_str()) || allow.contains(t.as_str()) {
                    return false;
                }
                let first_seg = t.split(['.', '[']).next().unwrap_or(t.as_str());
                !embedder.contains(first_seg)
            } else {
                !(top.contains(t.as_str())
                    || nested.contains(t.as_str())
                    || embedder.contains(t.as_str())
                    || allow.contains(t.as_str()))
            }
        })
        .cloned()
        .collect()
}

/// `embedderFieldShapes` documents its own invariant ("keep its key set mirroring `embedderFields`
/// exactly") — this is the machine pin for that mirror, so an agent adding an `embedderFields`
/// entry without a shape (or a shape for a dropped field) fails here instead of drifting silently.
/// The SHAPES' accuracy against `crates/facade/src/request.rs` stays a human/review concern (a
/// free-text one-liner has no mechanical truth source) — this test pins the key SET only.
#[test]
fn embedder_field_shapes_mirror_embedder_fields_exactly() {
    let vocab = load_config_surface();
    let fields: BTreeSet<&str> = vocab.embedder_fields.iter().map(String::as_str).collect();
    let shaped: BTreeSet<&str> = vocab
        .embedder_field_shapes
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        fields, shaped,
        "config-surface.json's embedderFieldShapes keys must mirror embedderFields exactly \
         (a field without a shape is a blind-agent dead-end; a shape without a field is stale)"
    );
    for (field, shape) in &vocab.embedder_field_shapes {
        assert!(
            !shape.trim().is_empty(),
            "embedderFieldShapes[{field}] is empty — an empty shape string documents nothing"
        );
    }
}
