//! Declared-response resolution (`response-shape-v1`) — `body_refs`'s sibling pass over
//! `IoProvide::response`, reading the SAME [`ShapeMerge`] so a name means one thing whether it types
//! a request body or a response. Two jobs, both never-guess:
//!
//! 1. **Ref resolution** — a `response.dto_ref` naming a merged class/interface shape gets its
//!    `fields`/`complete` copied and the ref cleared; a missing or poisoned name drops the whole
//!    `response` (one aggregated warning per `(file, ref)`, mirroring the body pass).
//! 2. **Undeclared-handler disclosure** — the parser's no-return-type SENTINEL (`dto_ref: None` +
//!    empty `fields`, see `zzop_core::ProvideResponseShape`'s doc) is stripped here and folded into
//!    ONE aggregated warning per tree naming the handler count and example files. This is the
//!    honesty half of the declaration-based contract: a handler that declares nothing is silent in
//!    the FACTS but never silently absent from the RUN — without the warning, "0 response findings"
//!    on an annotation-free codebase would be indistinguishable from a clean one (the same
//!    "key-present means it ran" stance `output-philosophy` §1 takes for coverage keys, and the same
//!    warnings channel the framework-silence self-reports use).
//!
//! An adapter-supplied `response` with `dto_ref: None` and NON-empty `fields` is already resolved
//! (Mode B overlays may fill fields directly, like `ProvideBodyShape`) and passes through untouched.
//!
//! 3. **Capture-less disclosure** (2026-08-03) — the third silence, one aggregated warning per tree
//!    over the `http` provides that arrive here with `response: None` AT ENTRY (before the sentinel
//!    strip and the unresolved-ref drops turn other provides into `None` too — the entry read is what
//!    keeps the three disclosures disjoint). A `None` here means NO response-shape evidence exists
//!    for that route, for one of two producer-side reasons the wire value cannot distinguish
//!    (`method_response_shape`'s outcome 3 shares the value with every capture-less producer): the
//!    route came from a provider shape with no response capture at all (Express/Hono router mounts,
//!    file-convention and pathname-dispatch routes, every non-TypeScript framework — today's only
//!    built-in capture is the Nest controller-decorator return-type read), or it is a Nest route
//!    whose annotation that capture cannot read (array/union/primitive/non-`Promise` generic —
//!    never-guess `None`). The warning names both causes so neither audience gets the OTHER's
//!    advice; what the causes share, and what the warning asserts, is that the response axis of
//!    those routes was NOT ANALYZED — no-evidence, never clean. Without it, a 100% Express `.ts`
//!    tree shows zero response findings AND zero disclosures (its extension is inside the sightline
//!    cover, so no blind-spot row appears either) — indistinguishable from a clean tree, the exact
//!    silence class the sentinel disclosure closed for Nest.

use std::collections::{BTreeMap, HashSet};

use zzop_core::IoProvide;

use super::shape_merge::ShapeMerge;

/// See the module doc. Must run AFTER every provide-composition pass, exactly like
/// `resolve_provide_body_refs` (the two are called back-to-back at the same seam).
pub(crate) fn resolve_provide_response_refs(
    io_provides: &mut [IoProvide],
    merge: &ShapeMerge,
    warnings: &mut Vec<String>,
) {
    let referenced: HashSet<&str> = io_provides
        .iter()
        .filter_map(|p| p.response.as_ref().and_then(|r| r.dto_ref.as_deref()))
        .collect();
    warnings.extend(merge.poisoned_disclosures(&referenced, "declared-response"));

    // Capture-less disclosure input, read AT ENTRY (module doc §3): `http` provides only — a
    // `db-table`/`trpc` provide has no response contract to capture, so counting it would inflate
    // the denominator with routes the axis never applies to.
    let total_http = io_provides.iter().filter(|p| p.kind == "http").count();
    let mut capture_less: BTreeMap<String, u32> = BTreeMap::new();
    for p in io_provides.iter() {
        if p.kind == "http" && p.response.is_none() {
            *capture_less.entry(p.file.clone()).or_insert(0) += 1;
        }
    }

    let mut unresolved: BTreeMap<(String, String), u32> = BTreeMap::new();
    // file -> distinct undeclared HANDLERS (`(line, symbol)` — one method is one entry no matter
    // how many provides it emits: an array-path decorator emits one sentinel per path from a
    // single annotatable method, and the disclosure counts what the developer can annotate).
    // BTreeMap for deterministic example order.
    let mut undeclared: BTreeMap<String, HashSet<(u32, Option<String>)>> = BTreeMap::new();

    for provide in io_provides.iter_mut() {
        let Some(resp) = provide.response.as_ref() else {
            continue;
        };
        let Some(dto_ref) = resp.dto_ref.clone() else {
            if resp.fields.is_empty() {
                // The no-return-type sentinel — strip it (a zero-information shape must not reach
                // rules, the join, or JSON output) and count its HANDLER for the disclosure.
                undeclared
                    .entry(provide.file.clone())
                    .or_default()
                    .insert((provide.line, provide.symbol.clone()));
                provide.response = None;
            }
            // Non-empty fields with no ref = adapter-resolved shape; leave untouched.
            continue;
        };
        if merge.is_poisoned(&dto_ref) {
            provide.response = None;
            *unresolved
                .entry((provide.file.clone(), dto_ref))
                .or_insert(0) += 1;
            continue;
        }
        match merge.get(&dto_ref) {
            Some(frag) => {
                if let Some(shape) = provide.response.as_mut() {
                    shape.fields = frag.fields.clone();
                    shape.complete = frag.complete;
                    shape.dto_ref = None;
                }
            }
            None => {
                provide.response = None;
                *unresolved
                    .entry((provide.file.clone(), dto_ref))
                    .or_insert(0) += 1;
            }
        }
    }

    for ((file, dto_ref), count) in unresolved {
        let provide_word = if count == 1 { "provide" } else { "provides" };
        warnings.push(format!(
            "could not resolve declared response type `{dto_ref}` ({file}) to a known class/interface \
             shape — its {count} {provide_word} keep no response contract; the type may live in an \
             unanalyzed file, or be a type alias/mapped type this declaration-based extraction does not \
             read"
        ));
    }

    if !undeclared.is_empty() {
        let total: usize = undeclared.values().map(HashSet::len).sum();
        let files = undeclared.len();
        let examples: Vec<&str> = undeclared.keys().take(3).map(String::as_str).collect();
        let handler_word = if total == 1 { "handler" } else { "handlers" };
        let file_word = if files == 1 { "file" } else { "files" };
        warnings.push(format!(
            "{total} route {handler_word} across {files} {file_word} (e.g. {}) declare no return type — \
             declared-response-shape analysis (`cross-layer/sensitive-response-field`, response-contract \
             checks) is off for those routes, never guessed from the handler body; declare a return type \
             (e.g. `Promise<SomeDto>`) to turn it on",
            examples.join(", ")
        ));
    }

    if !capture_less.is_empty() {
        let total: u32 = capture_less.values().sum();
        let examples: Vec<&str> = capture_less.keys().take(3).map(String::as_str).collect();
        let route_word = if total == 1 { "route" } else { "routes" };
        warnings.push(format!(
            "{total} of {total_http} http {route_word} in this tree (e.g. {}) carry no response-shape \
             evidence — each either comes from a provider shape with no response capture \
             (Express/Hono router mounts, file-convention and pathname-dispatch routes, every \
             non-TypeScript framework; today's only built-in capture is the Nest controller-decorator \
             return-type read) or carries a return annotation that capture cannot read (an \
             array/union/primitive/non-`Promise` generic — never guessed). Declared-response analysis \
             (`cross-layer/sensitive-response-field`, response-contract checks) never ran for those \
             routes: zero response findings there is no-evidence, never \"no sensitive response \
             field\". A Nest route turns it on with a readable return type (`Promise<SomeDto>`); \
             every other shape needs the fields supplied through a Mode B adapter overlay's \
             `response` — declaring a return type alone does not turn it on there",
            examples.join(", ")
        ));
    }
}

#[cfg(test)]
mod tests;
