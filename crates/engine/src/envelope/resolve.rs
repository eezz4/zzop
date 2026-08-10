//! Envelope-mode resolution helpers — the fragment `Ref`/`Mount` specifier resolver and the
//! SymbolScan/IoScan-only rule-pack filter, both consumed by `ingest::analyze_envelope`.

use std::collections::HashSet;

use zzop_core::{Matcher, RulePackDef};

/// Extensions appended, in order, to an extensionless relative join — the third of this repo's three
/// TypeScript-family extension lists, and the narrowest.
///
/// **Why this is not `dead_exports::is_ts_source_ext`'s eight**, stated here because the question is
/// otherwise unanswerable from the code: this list is not "files we can parse", it is "extensions a
/// resolver may INVENT when the specifier does not name one". `.mjs`/`.cjs`/`.mts`/`.cts` are never
/// reachable that way under any toolchain — Node's ESM resolver performs no extension search at all,
/// and TypeScript's `node16`/`nodenext` mode reaches a `.mts`/`.cts` file only through an explicit
/// `.mjs`/`.cjs` specifier (see `zzop_parser_typescript`'s own `.mjs`→`.mts` mapping, which is that
/// explicit path). Appending them here would resolve a mount that no build ever resolves. `.jsx` IS
/// searched by classic/bundler resolution and belongs — it was missing until 2026-07-29, so a
/// JS-flavored projection whose fragment `Ref` read `./Button` against a `Button.jsx` file silently
/// produced no mount. Pinned in both directions by this module's tests in
/// `super::tests::rules_and_diagnostics::resolve_envelope_specifier_tests`.
const EXTENSIONLESS_CANDIDATE_EXTENSIONS: [&str; 4] = [".ts", ".tsx", ".js", ".jsx"];

/// Resolves one fragment `Ref`/`Mount` specifier for envelope-mode composition — no tsconfig/
/// workspace-alias machinery, since an envelope's `FileProjection::path` set is the entire addressable
/// universe. Contract: (a) an exact match of `specifier` against known file paths wins outright; (b)
/// else, if `specifier` starts with `./` or `../`, join it against `from_file`'s own directory
/// (normalizing `.`/`..` segments as pure string ops, no filesystem APIs), try that joined path as-is,
/// then try appending each of [`EXTENSIONLESS_CANDIDATE_EXTENSIONS`] in turn; (c) anything else
/// resolves to `None` — external/unresolved, never guessed.
pub(super) fn resolve_envelope_specifier(
    specifier: &str,
    from_file: &str,
    all_paths: &HashSet<&str>,
) -> Option<String> {
    if all_paths.contains(specifier) {
        return Some(specifier.to_string());
    }
    if !specifier.starts_with("./") && !specifier.starts_with("../") {
        return None;
    }

    // `from_file`'s own directory, as path segments (envelope paths are contractually forward-slash,
    // so plain `/`-splitting avoids `std::path::Path`'s Windows-backslash normalization surprises).
    let mut segments: Vec<&str> = from_file.split('/').collect();
    segments.pop(); // drop the file's own basename, keeping just its directory

    for part in specifier.split('/') {
        match part {
            "." | "" => {}
            ".." => {
                segments.pop();
            }
            seg => segments.push(seg),
        }
    }
    let joined = segments.join("/");

    if all_paths.contains(joined.as_str()) {
        return Some(joined);
    }
    for ext in EXTENSIONLESS_CANDIDATE_EXTENSIONS {
        let candidate = format!("{joined}{ext}");
        if all_paths.contains(candidate.as_str()) {
            return Some(candidate);
        }
    }
    None
}

/// Whether envelope mode evaluates a rule with this matcher at all — see the envelope module doc for
/// why only `SymbolScan`/`IoScan` run. THE single definition of that filter: [`envelope_rule_pack`]
/// (evaluation) and `ingest`'s `compute_dsl_scope_filtered` call (the `zeroAdmissionRules` census)
/// both read it, so what the census reports as never-run and what evaluation actually drops cannot
/// drift apart.
pub(crate) fn rule_runs_in_envelope_mode(matcher: &Matcher) -> bool {
    matches!(matcher, Matcher::SymbolScan(_) | Matcher::IoScan(_))
}

/// `pack`, with every rule [`rule_runs_in_envelope_mode`] rejects dropped — see the envelope module
/// doc for why.
pub(super) fn envelope_rule_pack(pack: &RulePackDef) -> RulePackDef {
    let mut p = pack.clone();
    p.rules.retain(|r| rule_runs_in_envelope_mode(&r.matcher));
    if p.rules.len() != pack.rules.len() {
        // Same seam-hygiene as `gate_pack_rules`: a rules vec of a different shape must not share
        // the original's positional prefilter state (`RegexCache::fork_for_mutated_rules`).
        p.regex_cache = pack.regex_cache.fork_for_mutated_rules();
    }
    p
}
