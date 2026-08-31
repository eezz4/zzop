//! Java's half of the call-graph re-parse — the sibling of [`super::python_guard`] and
//! [`super::rust_guard`], and the first of the three lifts.
//!
//! Java is the language whose imports are re-parsed FRESH here rather than arriving pre-computed. TS's
//! and Python's reach this pass via `ts_import_pairs` off the fused per-file pass; no `java_import_pairs`
//! equivalent is threaded in, so parsing calls and imports together in one read keeps the Java side
//! self-contained instead of growing the caller's parameter list for a fact only this pass needs.
//!
//! Java text is deliberately NOT folded into the caller's `file_texts`: that map feeds the TS-shaped
//! `is_whitelisted` lookback and `extract_controller_guarded_lines`, neither of which finds anything
//! Java in it. Java's one `mutating-route-no-auth` signal — Spring method-security annotations — is read
//! here instead, into the Java half of the framework-neutral decorator-guard exemption set.

use std::collections::{HashMap, HashSet};

use zzop_core::ImportMap;

/// What the Java pass contributes beyond the calls and imports it appends to the caller's maps.
pub(super) struct JavaGuards {
    /// `(file, line)` pairs carrying Spring method security (`@PreAuthorize`/etc.) — the Java half of
    /// `ScanMutatingRouteNoAuthInput::decorator_guarded`.
    pub(super) decorator_guarded: HashSet<(String, u32)>,
    /// Spring Security global authorization postures (secure-by-default `authorizeRequests` chains), one
    /// per config file, collected across the tree. The caller applies them ONLY if exactly one exists —
    /// multiple means ambiguous scoping, unsafe to reason about, so they are left unapplied.
    pub(super) postures: Vec<(String, zzop_parser_java_21::SpringSecurityPosture)>,
}

/// Reads every Java file once: appends its calls to `raw_calls` and its imports to `imports_by_file`,
/// and returns the guard evidence above. Guard extraction is skipped entirely unless
/// `need_decorator_guarded` — the read and the call parse happen either way, since the graph needs them.
pub(super) fn parse_calls_and_guards(
    root: &std::path::Path,
    java_rels: &[String],
    need_decorator_guarded: bool,
    raw_calls: &mut Vec<zzop_core::callgraph::RawCall>,
    imports_by_file: &mut HashMap<String, ImportMap>,
) -> JavaGuards {
    let mut decorator_guarded: HashSet<(String, u32)> = HashSet::new();
    let mut postures = Vec::new();
    for rel in java_rels {
        let Ok(bytes) = std::fs::read(root.join(rel)) else {
            continue;
        };
        let text = String::from_utf8_lossy(&bytes).into_owned();
        raw_calls.extend(zzop_parser_java_21::parse_calls(rel, &text));
        imports_by_file.insert(rel.clone(), zzop_parser_java_21::parse_imports(&text));
        if need_decorator_guarded {
            for line in zzop_parser_java_21::extract_spring_guarded_lines(rel, &text) {
                decorator_guarded.insert((rel.clone(), line));
            }
            // `Err` is a NAMED bail (`zzop_parser_java_21::SpringPostureBail`) rather than a silent
            // `None`, so a later pass can hook the shape it knows how to resolve. Nothing consumes the
            // reason yet — the extractor's contract here is unchanged: no posture, no exemption.
            if let Ok(p) = zzop_parser_java_21::extract_spring_security_posture(rel, &text) {
                postures.push((rel.clone(), p));
            }
        }
    }
    JavaGuards {
        decorator_guarded,
        postures,
    }
}
