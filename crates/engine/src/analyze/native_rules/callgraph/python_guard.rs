//! Python's half of `run_callgraph_rules`' decorator-guard evidence — split out of `mod.rs` for the same
//! reason `decorator_gate` was: it is a self-contained gather with two shapes and its own conflict rule,
//! and inlining it would push the caller past this repo's file-size policy.
//!
//! The two producers it drives live in the parser (kernel-ignorance: framework vocabulary such as
//! `Depends`/`permission_classes` never enters the engine):
//! - `zzop_parser_python_3::extract_fastapi_guarded_lines` — FastAPI `Depends(...)` evidence, already
//!   keyed by the route decorator's own line, so it lands directly in the framework-neutral
//!   `(file, line)` `decorator_guarded` set the same way Spring's `@PreAuthorize` half does.
//! - `zzop_parser_python_3::extract_django_view_guard_classes` — Django REST Framework
//!   `permission_classes` verdicts, keyed by VIEW-CLASS NAME. Django splits the two halves across files
//!   (`views.py` holds the evidence, `urls.py` holds the route registration), so this one cannot be a
//!   `(file, line)` and is instead applied by [`apply_django_view_guards`] against each Python http
//!   provide's own `symbol` — which the URLconf scan already recorded as the view name.
//!
//! ## Two-phase, because a FastAPI guard alias crosses files
//! `extract_fastapi_guarded_lines` needs the TREE-WIDE set of `X = Annotated[..., Depends(<guard>)]`
//! alias names (the `CurrentUser` idiom: declared once in a shared `deps.py`, annotated in every route
//! module). So phase 1 RESOLVES `extract_python_guard_aliases` over every Python text and phase 2 judges
//! routes with that set in hand. Both phases read text already in memory — no extra file I/O.
//!
//! ## One conflict rule, applied to both by-NAME producers
//! Neither producer's output is keyed by file, so both are resolved by the same rule: a name two files
//! verdict DIFFERENTLY resolves to nothing (`merge_verdicts`/`resolve_guarded`). Each parser producer
//! therefore reports its non-guards too, `false`-verdicted — a producer that reported only its guards
//! would make the tree-wide set MONOTONE toward suppression, where any one file's guard declaration
//! clears every same-named use in the tree. Django's join carries a third case its verdicts cannot see
//! at all (a class that declares nothing); [`apply_django_view_guards`] answers that one.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use zzop_core::callgraph::RawCall;

/// Re-parses every Python-dispatched file's CALL SITES off disk and, when the caller needs guard
/// evidence, runs both guard producers over the same already-read text — the Python counterpart of
/// `run_callgraph_rules`' TS and Java loops. Python-dispatched files ride the shared, TS-named `ts_paths`
/// set (`pipeline::fresh`'s `ts_slot`) and their `ImportMap`s already ride `ts_import_pairs`, so ONLY the
/// call sites need re-reading here (unlike Java, whose imports are re-parsed too). `rels` is sorted so
/// the two guard phases are independent of `ts_paths`' hash iteration order.
pub(super) fn parse_calls_and_guards(
    root: &std::path::Path,
    ts_paths: &HashSet<String>,
    need_guards: bool,
    vocab: &zzop_parser_python_3::PythonGuardVocab<'_>,
    raw_calls: &mut Vec<RawCall>,
) -> PythonGuards {
    let mut rels: Vec<&String> = ts_paths
        .iter()
        .filter(|rel| crate::analyze::assemble::helpers::is_python_source_ext(rel))
        .collect();
    rels.sort();
    let mut texts: Vec<(&str, String)> = Vec::new();
    for rel in rels {
        if let Ok(bytes) = std::fs::read(root.join(rel)) {
            let text = String::from_utf8_lossy(&bytes).into_owned();
            raw_calls.extend(zzop_parser_python_3::parse_calls(rel, &text));
            texts.push((rel.as_str(), text));
        }
    }
    if need_guards {
        collect(&texts, vocab)
    } else {
        PythonGuards::default()
    }
}

/// A Python call's cross-file target file — the REAL module resolver, unlike Java's opaque stand-in.
/// Runs `zzop_parser_python_3::python_import_candidates` against the tree's own path set (the same
/// `assemble::helpers::resolve_python_import` glue the dep-graph and `include_router` composition use)
/// with `original: None`: a CALL's target is a function/class INSIDE the named module, so the plain
/// `<base>.py` / `<base>/__init__.py` candidates are the right ones, while the submodule-first shape
/// `original` unlocks would name a module, not a callee. A specifier resolving to no in-tree file yields
/// `None` and the edge is dropped, never guessed.
///
/// Known limitation, same class as Java's: a MODULE-attribute receiver (`from pkg import mod; mod.f()`)
/// resolves as if `mod` were a class, so the edge target id is a node nothing else has outgoing edges
/// from — a second hop through it is not found. Single-hop is the coverage this wiring buys.
pub(super) fn resolve_python_call_target(
    specifier: &str,
    from_file: &str,
    ts_paths: &HashSet<String>,
    python_package_roots: &[&str],
) -> Option<String> {
    crate::analyze::assemble::helpers::resolve_python_import(
        specifier,
        None,
        from_file,
        ts_paths,
        python_package_roots,
    )
}

/// What [`collect`] gathers: the directly-anchored FastAPI lines, plus the by-name Django verdicts that
/// still need a provide to attach to.
#[derive(Default)]
pub(super) struct PythonGuards {
    /// Route-registration `(file, line)` pairs — merged straight into `decorator_guarded`.
    pub(super) guarded_lines: HashSet<(String, u32)>,
    /// View-class names whose DRF `permission_classes` names a guard, after the conflict drop below.
    pub(super) guarded_view_classes: BTreeSet<String>,
}

/// Runs both Python guard producers over already-read file texts — module doc. `texts` is expected in a
/// deterministic order (the caller sorts by rel), though every output here is a set and so is
/// order-independent anyway.
fn collect(
    texts: &[(&str, String)],
    vocab: &zzop_parser_python_3::PythonGuardVocab<'_>,
) -> PythonGuards {
    // Phase 1 — the tree-wide by-NAME verdict maps. BOTH producers are joined by name across files, so
    // both get the same conflict rule: `None` marks a name two files disagree about, and a disagreeing
    // name resolves to nothing rather than to whichever file was read first. Silently picking either one
    // would suppress a real finding half the time — the same "ambiguous resolves to nothing" discipline
    // `resolve_handler_scoped` applies.
    let mut alias_verdicts: BTreeMap<String, Option<bool>> = BTreeMap::new();
    let mut view_verdicts: BTreeMap<String, Option<bool>> = BTreeMap::new();
    for (_, text) in texts {
        merge_verdicts(
            &mut alias_verdicts,
            zzop_parser_python_3::extract_python_guard_aliases_with_vocab(text, vocab),
        );
        merge_verdicts(
            &mut view_verdicts,
            zzop_parser_python_3::extract_django_view_guard_classes_with_vocab(text, vocab),
        );
    }
    let aliases = resolve_guarded(alias_verdicts);

    // Phase 2 — judge each file's routes with the resolved alias set in hand.
    let mut guarded_lines = HashSet::new();
    for (rel, text) in texts {
        for line in zzop_parser_python_3::extract_fastapi_guarded_lines_with_vocab(
            rel, text, &aliases, vocab,
        ) {
            guarded_lines.insert(((*rel).to_string(), line));
        }
    }

    PythonGuards {
        guarded_lines,
        guarded_view_classes: resolve_guarded(view_verdicts),
    }
}

/// Folds one file's `(name, guarded)` verdicts into the tree-wide map — see [`collect`]'s conflict rule.
fn merge_verdicts(into: &mut BTreeMap<String, Option<bool>>, from: Vec<(String, bool)>) {
    for (name, guarded) in from {
        into.entry(name)
            .and_modify(|prev| {
                if *prev != Some(guarded) {
                    *prev = None;
                }
            })
            .or_insert(Some(guarded));
    }
}

/// The names the tree agrees are guarded — every `None` (disagreement) and every `Some(false)` dropped.
fn resolve_guarded(verdicts: BTreeMap<String, Option<bool>>) -> BTreeSet<String> {
    verdicts
        .into_iter()
        .filter_map(|(name, verdict)| (verdict == Some(true)).then_some(name))
        .collect()
}

/// Applies the by-name Django verdicts: a Python-file `http` provide whose `symbol` names a guarded view
/// class is exempt at its own registration `(file, line)` — module doc. Extension-gated so a same-named
/// class in another language's tree can never clear a route the Python scan never looked at.
///
/// ## The name must also be UNIQUE in the tree, and `all_symbols` is what knows that
/// `collect`'s conflict rule can only see a name two files DECLARE `permission_classes` for. A view class
/// that declares none is (correctly) absent from the producer's output entirely — absence of the
/// attribute is not evidence of absence of auth — so it never registered as a conflict, and the one app
/// that DID declare a guard exempted every same-named view in the tree. Django's default layout makes
/// that collision ordinary: `UserDetailView`/`ProfileView` per app is the convention, and the join key is
/// the bare class name because the provide lives in `urls.py` while the evidence lives in `views.py`.
/// `all_symbols` already carries every file's declared class names, so a name declared as a class in 2+
/// Python files is ambiguous and clears nothing — the same discipline as the disagreement drop, applied
/// to the case where one side is silent rather than opposed.
pub(super) fn apply_django_view_guards(
    io_provides: &[zzop_core::IoProvide],
    guarded_view_classes: &BTreeSet<String>,
    all_symbols: &[zzop_core::SourceSymbol],
    decorator_guarded: &mut HashSet<(String, u32)>,
) {
    if guarded_view_classes.is_empty() {
        return;
    }
    let ambiguous = ambiguous_python_class_names(all_symbols);
    for p in io_provides.iter().filter(|p| p.kind == "http") {
        if !crate::analyze::assemble::helpers::is_python_source_ext(&p.file) {
            continue;
        }
        let Some(symbol) = p.symbol.as_deref() else {
            continue;
        };
        if guarded_view_classes.contains(symbol) && !ambiguous.contains(symbol) {
            decorator_guarded.insert((p.file.clone(), p.line));
        }
    }
}

/// Class names declared in 2+ distinct Python files — see [`apply_django_view_guards`]'s doc.
fn ambiguous_python_class_names(all_symbols: &[zzop_core::SourceSymbol]) -> BTreeSet<&str> {
    let mut files_by_name: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for s in all_symbols.iter().filter(|s| {
        s.kind == zzop_core::SourceSymbolKind::Class
            && crate::analyze::assemble::helpers::is_python_source_ext(&s.file)
    }) {
        files_by_name
            .entry(s.name.as_str())
            .or_default()
            .insert(s.file.as_str());
    }
    files_by_name
        .into_iter()
        .filter_map(|(name, files)| (files.len() > 1).then_some(name))
        .collect()
}
