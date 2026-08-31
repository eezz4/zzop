//! `duplicate-route` — same (METHOD, path) HTTP route provided 2+ times across the tree. The engine collects
//! `io_provides` and handles gating; this module only groups and reports.
//!
//! Groups whole-tree `IoProvide`s (`kind == "http"`) by normalized `http_interface_key` and flags every key
//! registered at 2+ distinct `(file, line)` sites, sorted for determinism: the first site is canonical, and
//! every later site gets one `Finding` naming it. This whole-tree pass is recomputed on every `analyze_tree`
//! call rather than cached, since its output is never read back out of a stale per-file cache entry.
//!
//! Provider sites in test-path files (`zzop_core::is_test_file`) are skipped — an isolated test
//! fixture's route never coexists with the "duplicate" at runtime.
//!
//! ## The condition this rule cannot check, and why the message leads with it
//! "Registered twice" only means "shadowed" if both registrations reach the SAME router in the SAME
//! process. This rule has no evidence about that: it groups the whole tree into one route table, and a
//! tree is a directory, not a deployment unit. Measured (2026-08-16, an external 1,705-file monorepo): a
//! shared registration helper in `base/utils-be` called by four services, plus a fifth service's own
//! registration, produced a flat "later registrations are shadowed — merge the handlers", which was
//! false on all five (separate processes) and destructive if followed (four services break). The
//! CROSS-TREE twin (`zzop_rules_cross_layer::cross_layer_duplicate_route_findings`) already states the
//! condition and names the "intentionally separate services" case — and it is in the STRONGER position
//! of the two, since a `trees[]` entry is a deployment unit the user declared. So the weaker rule must
//! hedge at least as hard, and its message points at the fix that actually resolves the ambiguity
//! (analyze the units as separate trees).
//!
//! That message used to point there INSTEAD of at suppression, and 2026-08-20 measured why that was
//! half a prescription: on macrozheng/mall the split it recommends cuts 410 cross-module import edges
//! into a 258-file shared artifact, leaving that artifact with no in-tree consumers, so an auditor who
//! priced the advice turned the rule off — the one thing the message told them not to do, and it was
//! the right call. A prescription that costs more than the defect makes the reader disobey it while
//! believing they are the ones at fault. So the message now names BOTH exits and prices them, and the
//! disable hint covers both reasons rather than only the within-one-process one.
//!
//! ## Manifest boundaries ride along, never clear — and since 2026-08-20 they move the SEVERITY
//! The tree DOES carry one cheap hint about deployment: the nearest directory above each site holding a
//! manifest that declares a deployable unit (`boundary::is_deployment_manifest` is the one place that
//! says which files those are). It is attached to a straddling finding as `data.manifestBoundaries` plus
//! a message sentence, and is never used to drop one — the ruling behind that asymmetry is `.claude`
//! §24: a finding raised on a heuristic is dismissed in seconds, a finding DELETED by one is a real
//! shadow that leaves no trace, so the erasing direction requires a DECLARATION and `package.json`
//! declares packaging rather than a process (a shared package can register routes an app package
//! mounts — two boundaries, one process).
//!
//! What that asymmetry did NOT settle is which side of the CI gate such a finding belongs on, and
//! leaving it at `warning` put the whole cost of the missing evidence on the reader: `--fail-on
//! warning` reads the counts, so every straddling pair broke a build over the one condition this rule
//! says it cannot check. A straddling pair is therefore `Severity::Info` — raised, named, out of the
//! gate — while a same-boundary or unmeasured pair keeps `Warning`. `boundary.rs` §"The third
//! direction" owns the measurement that moved it (mall 0504e86b, 7 of 7 false) and the reason this is
//! not a quiet re-run at the erasing direction.
//!
//! Absence of the field means "same boundary, or not measured", never "same" — and "not measured" is a
//! real state: an ecosystem whose deployable unit is declared in a file the vocabulary does not carry
//! contributes no boundaries at all, which the disclosure states rather than leaving as silence, and
//! which is why an unmeasured side never buys the demotion. See `boundary.rs`.
//!
//! ## Version scopes are the SECOND axis, and they move the LEAD as well as the severity
//! The group key is `"METHOD /path"` and nothing else, which is blind to every framework that versions
//! by something the URL never carries. NestJS's `VersioningType.HEADER`/`CUSTOM` is that shape:
//! measured 2026-08-21, all 15 cal.com findings were three `@Controller({path, version})` pairs over a
//! `cal-api-version` header, every one of them under ONE `package.json` so the manifest axis could not
//! reach them. When both sites carry an `IoProvide::route_version` and the two DIFFER, `version`
//! appends its sentence, sets `data.routeVersions`, and demotes to `info` — composing with the
//! manifest axis, never overwriting it. What is different from that axis is that this one also changes
//! the message's opening sentence (see `lead`): the default lead's "merge the handlers or remove the
//! duplicate" is not merely unhelpful on a versioned split but destructive, and a hedge appended 2,000
//! characters later does not undo an imperative the reader already acted on. Absence is "not measured"
//! and an identical version discloses nothing, for the reasons `version` states.
//!
//! A later site is skipped when it resolves to the same handler DECLARATION as the canonical site — same
//! `file` AND same non-empty `symbol`, see `same_registered_handler`, which owns why the file is half of
//! that key. One handler deliberately registered on two paths that normalize to one key is the
//! trailing-slash-tolerance idiom (e.g. gin's `router.POST("", h)` + `router.POST("/", h)`, so both `/x`
//! and `/x/` hit `h`), not a shadow — the same handler runs either way, so there is no "which handler
//! wins?" ambiguity for the rule to warn about. Anything short of that is still flagged: a different
//! declaration, a same-NAMED declaration in another file, or an unknown/empty symbol on either side where
//! sameness cannot be proven at all. That is the genuine shadowing case the rule exists for.

pub fn duplicate_route_findings(
    io_provides: &[zzop_core::IoProvide],
    manifest_dirs: &std::collections::BTreeSet<String>,
) -> Vec<zzop_core::Finding> {
    let mut by_key: std::collections::BTreeMap<&str, Vec<&zzop_core::IoProvide>> =
        std::collections::BTreeMap::new();
    for p in io_provides {
        if p.kind == "http" && !zzop_core::is_test_file(&p.file) {
            by_key.entry(p.key.as_str()).or_default().push(p);
        }
    }

    let mut findings = Vec::new();
    for (key, mut sites) in by_key {
        if sites.len() < 2 {
            continue;
        }
        sites.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
        let first = sites[0];
        for dup in &sites[1..] {
            // Same-handler multi-registration (trailing-slash tolerance / framework convention) is not a
            // shadow — skip it. Only a proven-same DECLARATION counts; anything short of that leaves the
            // conservative warning in place. See `same_registered_handler`.
            if same_registered_handler(first, dup) {
                continue;
            }
            let (boundary_note, boundary_data) =
                match boundary::disclosure(&first.file, &dup.file, manifest_dirs) {
                    Some((s, d)) => (Some(s), Some(d)),
                    None => (None, None),
                };
            // The SECOND non-erasing axis, and it COMPOSES with the first rather than replacing it:
            // a pair can straddle a manifest boundary AND split by version, and a reader who was
            // already relying on the boundary sentence must not lose it. See `version`.
            let (version_note, version_data) = match version::disclosure(first, dup) {
                Some((s, d)) => (Some(s), Some(d)),
                None => (None, None),
            };
            // The LEAD changes on the version axis, and that is the point of the axis rather than a
            // decoration on it. The default lead's imperative — "merge the handlers or remove the
            // duplicate" — sits at roughly character 330 of a 2,600-character message, ahead of every
            // hedge, and on a versioned split it is both wrong and destructive: merging is
            // inexpressible when the input DTOs differ BY DESIGN, and removing the older controller
            // breaks every client pinned to it. A hedge appended after 2,000 characters does not undo
            // an imperative the reader already acted on, so when a version discriminator separates the
            // two sites the reader must not meet that sentence at all. Both disclosure sentences then
            // append AFTER the shared tail, in a fixed order so the text is deterministic.
            let mut message = lead(key, first, version_note.is_some());
            message.push_str(&format!(
                    "That condition is exactly what this rule CANNOT check: it groups every http route in \
                     the analyzed tree into one table, and a tree is a directory, not a deployment unit. A \
                     monorepo holding several services — or a shared registration helper that each service \
                     calls once — puts these sites in separate processes, where nothing is shadowed and \
                     merging them would break every service but one. If that is your layout there are two \
                     ways out, and PRICE THEM BOTH: analyzing the units as separate `trees[]` entries is \
                     the declaration this rule honors, and it moves the same question to \
                     `cross-layer/duplicate-route`, where each providing source is known and named — but \
                     a split also cuts every import edge that crosses the units, so a shared in-repo \
                     library ends up with no in-tree consumers (measured on a four-application Maven \
                     monorepo: 410 cross-module edges into a 258-file shared artifact). Where that trade \
                     is not worth paying, turning this rule off is the honest second answer and not a \
                     failure to take the first. {} — for EITHER reason: the repeat is intentional within \
                     one process (a framework convention that legitimately registers the same route \
                     twice), or the units genuinely ship apart and the split above costs your other \
                     analyses more than these findings are worth.",
                    zzop_core::disable_hint("duplicate-route")
            ));
            message.push_str(boundary_note.as_deref().unwrap_or(""));
            message.push_str(version_note.as_deref().unwrap_or(""));

            findings.push(zzop_core::Finding {
                rule_id: "duplicate-route".to_string(),
                // A pair MEASURED to straddle two deployment manifests, or to declare two different
                // version scopes, drops out of the `--fail-on warning` gate — see `boundary`'s "third
                // direction" section for the measurement that moved it, `version` for the second axis,
                // and each module's `disclosure` for the sentence that says so in the finding itself.
                // Absence on BOTH axes keeps the warning: absence is "not measured".
                severity: match (&boundary_note, &version_note) {
                    (Some(_), _) | (_, Some(_)) => zzop_core::Severity::Info,
                    (None, None) => zzop_core::Severity::Warning,
                },
                file: dup.file.clone(),
                line: dup.line,
                message,
                // The first registration, which this message names by path.
                evidence_paths: if first.file == dup.file {
                    Vec::new()
                } else {
                    vec![first.file.clone()]
                },
                data: Some({
                    let mut d = serde_json::json!({
                        "key": key,
                        "first": {"file": first.file, "line": first.line},
                        "sites": sites.len(),
                    });
                    // Present only when the two sites were MEASURED to sit in different manifest
                    // boundaries. Absent means "same boundary, or not measured" — never "same".
                    if let Some(v) = boundary_data.clone() {
                        d["manifestBoundaries"] = v;
                    }
                    // Present only when both sides named a version and the two DIFFER. Its own key,
                    // beside the manifest one rather than instead of it — the two axes are
                    // independent and a pair can carry both. See `version`.
                    if let Some(v) = version_data.clone() {
                        d["routeVersions"] = v;
                    }
                    d
                }),
            });
        }
    }
    findings
}

/// The message's opening sentence — the only part of it that differs between the two axes, and the
/// part a reader actually acts on.
///
/// The DEFAULT lead states the shadowing claim as a condition and then prescribes the fix for it. The
/// VERSIONED lead exists because that prescription is not merely unhelpful on a version split, it is
/// destructive: `merge the handlers` is inexpressible when the two controllers take different input
/// DTOs by design, and `remove the duplicate` breaks every client pinned to the older version. Both
/// leads then run into the same tail, which prices the tree-split and the disable exits.
///
/// The shadowing claim survives in the versioned lead, just correctly quantified: zzop reads the
/// version EXPRESSION and cannot prove two different texts name disjoint sets, so an overlap is still
/// possible and is still the thing a reader should check. What changes is which action the reader is
/// pointed at first — verify the scopes, not delete a handler.
fn lead(key: &str, first: &zzop_core::IoProvide, versioned: bool) -> String {
    let (file, line) = (&first.file, first.line);
    if versioned {
        return format!(
            "route `{key}` is registered more than once (first at {file}:{line}) — but the two sites \
             declare DIFFERENT version scopes, so DO NOT merge them or delete either handler on the \
             strength of this finding: a versioned split is deliberate, the two contracts differ by \
             design, and removing one breaks every client pinned to it. Check instead whether the two \
             scopes can OVERLAP: only if they do, AND both registrations end up on the same router in \
             the same running process, is the later one shadowed or ambiguous depending on the \
             framework. "
        );
    }
    format!(
        "route `{key}` is registered more than once (first at {file}:{line}) — IF both registrations \
         end up on the same router in the same running process, the later one is shadowed or \
         ambiguous depending on the framework, and which handler wins is a deploy-order accident \
         rather than a design decision; merge the handlers or remove the duplicate. "
    )
}

/// Whether two registrations of one route key resolve to the SAME handler declaration — the only thing
/// the trailing-slash-tolerance skip above is allowed to mean.
///
/// **The comparison key is `(file, symbol)`, never `symbol` alone.** `IoProvide.symbol` is a bare
/// declaration name (`method.name` for a Spring/NestJS/ASP.NET controller member), which is the exact
/// hazard `VERSIONING.md` states about `SourceSymbol.id`: NOT UNIQUE, *"treat it as a LABEL, not a
/// key"* — comparing on it collapses colliding siblings silently, and here the collapse deleted a
/// routing verdict rather than a lookup. Two controllers in two files that both spell their handler
/// `list` are two different handlers, and `list`/`create`/`update`/`delete` are the most common handler
/// names there are, so a bare-name skip was weakest exactly where duplicates are most likely. Measured
/// on macrozheng/mall: `GET /order/list` is registered by `OmsOrderController.list` and
/// `OmsPortalOrderController.list`, and the bare-name skip dropped that finding in silence.
///
/// **Why `file` is the other half.** The idiom this skip exists for is ONE handler registered twice, and
/// both of those registrations are written at one registration site: gin's `router.POST("", h)` and
/// `router.POST("/", h)` sit in the same file, as do two branches of a pathname dispatcher that fall
/// back to their enclosing function. Pairing the name with the file is the disambiguating spelling the
/// data already carries — `IoProvide` holds nothing else that separates two same-named handlers.
///
/// **What it still cannot separate**, stated rather than left as silence: two same-named declarations
/// inside ONE file (two classes in one Java file, two nested TypeScript scopes) still compare equal and
/// stay silent. That is the conservative direction for a predicate whose only power is to REMOVE a
/// warning, and a strictly smaller blind spot than the whole-tree name match it replaces.
fn same_registered_handler(a: &zzop_core::IoProvide, b: &zzop_core::IoProvide) -> bool {
    match (a.symbol.as_deref(), b.symbol.as_deref()) {
        (Some(x), Some(y)) => !x.is_empty() && x == y && a.file == b.file,
        _ => false,
    }
}

pub mod boundary;
mod version;

#[cfg(test)]
mod tests;
