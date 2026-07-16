//! Envelope-mode resolution helpers — the fragment `Ref`/`Mount` specifier resolver and the
//! SymbolScan/IoScan-only rule-pack filter, both consumed by `ingest::analyze_envelope`.

use std::collections::HashSet;

use zzop_core::{Matcher, RulePackDef};

/// Resolves one fragment `Ref`/`Mount` specifier for envelope-mode composition — no tsconfig/
/// workspace-alias machinery, since an envelope's `FileProjection::path` set is the entire addressable
/// universe. Contract: (a) an exact match of `specifier` against known file paths wins outright; (b)
/// else, if `specifier` starts with `./` or `../`, join it against `from_file`'s own directory
/// (normalizing `.`/`..` segments as pure string ops, no filesystem APIs), try that joined path as-is,
/// then try appending each of `.ts`/`.tsx`/`.js` in turn; (c) anything else resolves to `None` —
/// external/unresolved, never guessed.
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
    for ext in [".ts", ".tsx", ".js"] {
        let candidate = format!("{joined}{ext}");
        if all_paths.contains(candidate.as_str()) {
            return Some(candidate);
        }
    }
    None
}

/// `pack`, with every rule whose matcher is not `SymbolScan`/`IoScan` dropped — see the envelope
/// module doc for why.
pub(super) fn envelope_rule_pack(pack: &RulePackDef) -> RulePackDef {
    let mut p = pack.clone();
    p.rules
        .retain(|r| matches!(r.matcher, Matcher::SymbolScan(_) | Matcher::IoScan(_)));
    p
}
