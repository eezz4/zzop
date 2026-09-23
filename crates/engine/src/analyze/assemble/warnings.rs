//! Phase 5: BE-framework coverage self-report (`crate::framework_silence`'s tripwire family) —
//! flags a tree that LOOKS like it carries a framework surface zzop cannot see. Split out of
//! `super::assemble` as its own phase since the tripwires share the same `io_provides`/
//! `io_consumes`/`ts_paths`/`java_rels`/`package_import_files` inputs and are otherwise independent of
//! every other `assemble` phase.

use std::collections::BTreeMap;

mod provide_side;
mod wrong_key;

/// Runs every framework-silence tripwire and returns each warning that fired, in the push order the
/// calls below appear in (S6 slotted after S4 at introduction; S8 after S6, before the S3/S5/S7
/// precheck block, since it needs no precheck; S3/S5 keep their pre-split tail positions; S7 slotted
/// after S5, sharing S5's precheck block; later ids are appended where their INPUT is in scope rather
/// than in id order — S16 sits after S2 and S17 after S8, each beside the sibling it mirrors). Neither the
/// count NOR the id range is written here — this doc said "seven, S1-S7" in one sentence and "eight,
/// S1-S8" in the next while thirteen ran, and then said "S9-S13" after S14 landed. The roster is
/// `crate::framework_silence`'s to state, and that module now declines to state it too: each tripwire
/// file carries its own `//! S<n>` header, which is the only account that cannot drift. Order matters for
/// `AnalyzeOutput::warnings`' documented stability, not correctness (each tripwire is independent). S5
/// and S7 are now per-app censuses: each may contribute MULTIPLE entries (one per below-floor app-root,
/// in sorted `app_roots` order) plus an optional tree-wide fallback, all of S5's before all of S7's.
/// S15 is appended LAST and is the one entry that detects nothing: it rides whichever of the others
/// fired, so it can only be decided once they all have.
#[allow(clippy::too_many_arguments)]
pub(super) fn framework_silence_warnings(
    root: &std::path::Path,
    io_provides: &[zzop_core::IoProvide],
    io_consumes: &[zzop_core::IoConsume],
    ts_paths: &std::collections::HashSet<String>,
    java_rels: &[String],
    csharp_rels: &[String],
    package_import_files: &BTreeMap<String, std::collections::BTreeSet<String>>,
    loc_by_path: &std::collections::HashMap<String, u32>,
    // S17's measured half — extension -> structurally-read file count. Every other tripwire here keys
    // on what was FOUND, which is exactly why they all go quiet when nothing was.
    structural_by_ext: &BTreeMap<String, usize>,
    // The run's declared `vocabulary.fetchWrapperExportNames` — S7's wrapper-module recognizer.
    wrapper_export_names: &[&str],
    // The run's rule gate — S15's ONLY use, and the reason this function takes config at all: its
    // disclosure NAMES rule ids, and naming a rule the user switched off is a worse answer than
    // naming none. Deliberately `&RuleConfig` and not the whole `EngineConfig`: no other tripwire
    // here reads config, so widening the parameter would advertise a coupling that does not exist.
    rule_gate: &zzop_core::RuleConfig,
    // What adapter overlays contributed. Every tripwire here asks "can zzop SEE this framework?", so
    // each one must judge on the count zzop extracted — a merged total answers a different question
    // ("does this tree have route visibility?") and answering the first with the second is what let an
    // overlay silence the very warning that asked for it. See `diagnostics::overlay_provenance`.
    overlay_io: &BTreeMap<String, crate::envelope::OverlayIoCounts>,
    // The lexically-visible route-registration count, set only when the provide-side trio measured it
    // (S2's precondition). Rides out of this phase because its second reader is the run-wide
    // provide-blind severity gate, which cannot re-derive the file set without risking a different
    // population than the extractor saw — see `provide_side_warnings`.
    visible_route_registrations_out: &mut Option<usize>,
) -> Vec<String> {
    let mut warnings = Vec::new();

    // BE-framework coverage self-report (`crate::framework_silence`): flags a tree that looks like it
    // has a backend but produced zero `http` provides — an unsupported/unrecognized framework signal
    // (S1). Computed here, while `io_provides`/`io_consumes`/`ts_paths`/`java_rels`/`package_import_files`
    // are still in scope.
    let http_count = crate::analyze::diagnostics::native_http_provides(
        io_provides.iter().filter(|p| p.kind == "http").count(),
        overlay_io,
    );
    let mut candidate_rels: Vec<String> = ts_paths.iter().cloned().collect();
    candidate_rels.extend(java_rels.iter().cloned());
    candidate_rels.sort();
    candidate_rels.dedup();
    // `provide_side_alarm`/`consume_side_alarm` below record WHICH sibling already told the user a
    // channel came up empty — S15 at the bottom rides them rather than gating itself, because "the
    // channel is empty" and "an empty channel here is a surprise" are different judgments and the
    // siblings own the second one. A frontend tree with no routes is not a gap.
    // S1 · S2 · S16 — the provide-side tripwire trio. They live in their own module because they are
    // the only three that read `candidate_rels` from DISK, and because this file crossed the 300-line
    // cap when S2 grew a fourth argument (2026-09-06). Splitting on that seam rather than at an
    // arbitrary line keeps one question in one file: "does this tree serve routes zzop cannot see?"
    let provide_side_alarm = provide_side::provide_side_warnings(
        root,
        &candidate_rels,
        package_import_files,
        io_provides,
        http_count,
        &mut warnings,
        visible_route_registrations_out,
    );
    // S4 — http-client import tripwire (consume side): an http-CLIENT package import present while
    // extracted `http` consumes stay near-zero — the consume-side dual of S2. Additive to S1-S3 above;
    // any subset may fire together. `http_consumes_count` counts ALL extracted `http`-kind consume
    // records — keyed AND unresolved — per `client_library_import_warning`'s own doc on why. Pure map
    // lookup over `package_import_files`, no disk IO, so unconditional.
    let http_consumes_count = crate::analyze::diagnostics::native_http_consumes(
        io_consumes.iter().filter(|c| c.kind == "http").count(),
        overlay_io,
    );
    let mut consume_side_alarm = false;
    if let Some(w) = crate::framework_silence::client_library_import_warning(
        package_import_files,
        http_consumes_count,
    ) {
        consume_side_alarm = true;
        warnings.push(w);
    }

    // S6 — ORM-schema silence tripwire (db-table channel): an ORM-schema package/import (TypeORM,
    // Sequelize, Drizzle, JPA, SQLAlchemy, GORM) present while zero `db-table` io facts (provides PLUS
    // consumes, tree-wide) were extracted — EXACT zero, not near-zero (see `orm_schema_silence_warning`'s
    // own doc for why). Pure map lookup over `package_import_files`, no disk IO, so unconditional.
    let db_table_fact_count = io_provides.iter().filter(|p| p.kind == "db-table").count()
        + io_consumes.iter().filter(|c| c.kind == "db-table").count();
    if let Some(w) = crate::framework_silence::orm_schema_silence_warning(
        package_import_files,
        db_table_fact_count,
    ) {
        warnings.push(w);
    }

    // S8 — call-graph language-coverage self-report: this tree extracted http routes from a language
    // whose call sites no parser in this build produces, so `mutating-route-no-auth` is structurally
    // silent there. Unlike S1-S7 this one fires on FULL channels (see its own module doc), and it is a
    // pure pass over `io_provides` — no disk IO, so unconditional.
    if let Some(w) = crate::framework_silence::call_graph_language_gap_warning(io_provides) {
        warnings.push(w);
    }

    // S17 — S8's MIRROR, and the case S8 structurally cannot reach: S8 iterates the routes that WERE
    // extracted, so a language this build finds none of leaves it silent exactly where the gap is
    // largest (gogs: ~300 macaron registrations, 0 extracted, `warnings` byte-identical).
    // Skipped when a sibling already reported this tree's empty provide channel: S1/S2 name the
    // FRAMEWORK, which is the better answer, and two warnings for one silence read as two problems —
    // the same carve-out S16 makes. And it does NOT set the alarm: S15 rides that flag to name the
    // rules an empty channel silences, and S8/S9/S10/S11 (the family this mirrors) all leave it alone.
    if !provide_side_alarm {
        if let Some(w) = crate::framework_silence::route_language_zero_extraction_warning(
            structural_by_ext,
            io_provides,
            io_consumes,
        ) {
            warnings.push(w);
        }
    }

    // S9 — method-unknown route RANGE self-report. Same shape as S8 and the opposite gap: these routes
    // ARE in a call-graph-covered language, they just carry no verb, so every write-gated rule filters
    // them out before evaluating. Also a pure pass over `io_provides`, so also unconditional.
    if let Some(w) = crate::framework_silence::unknown_verb_range_warning(io_provides) {
        warnings.push(w);
    }

    // S10 — Rust router-layer auth RANGE self-report. Third of the same family and the one D18 created:
    // lifting `.rs` into the call graph put Rust routes in `mutating-route-no-auth`'s range, which made
    // the one auth idiom this engine cannot see (a tower layer) start costing false positives. Also a
    // pure pass over `io_provides`, so also unconditional.
    if let Some(w) = crate::framework_silence::rust_router_layer_warning(io_provides) {
        warnings.push(w);
    }

    // S18 — protected-path auth RANGE self-report: S10's twin one rule over (same family, different
    // consumer — see its own module doc for the three idioms and the tree that measured them). Takes
    // `rule_gate` for S15's reason (it NAMES a rule id); otherwise a pure `io_provides` pass, so also
    // unconditional.
    if let Some(w) =
        crate::framework_silence::protected_path_auth_range_warning(io_provides, rule_gate)
    {
        warnings.push(w);
    }

    // S11 — unread io KIND self-report. The others are about what could not be EXTRACTED; this one is
    // about facts that were extracted fine and that nothing in this build reads. `IoKind` is an open
    // String, so a Mode B adapter can emit `"queue"` today and get facts in, zero findings out, with the
    // kind-agnostic coverage census confirming the channel filled. Pure pass over both io slices.
    if let Some(w) = crate::framework_silence::unread_io_kind_warning(io_provides, io_consumes) {
        warnings.push(w);
    }

    // S12/S13/S14 — the WRONG-KEY family, in their own module. The seam is the one their own comments
    // already drew: every other tripwire here reports a SILENCE, while these three report a plausible
    // FINDING produced by a prefix this build never read.
    warnings.extend(wrong_key::wrong_key_warnings(
        root,
        io_provides,
        csharp_rels,
        loc_by_path,
    ));

    // S3/S5/S7 prechecks. S3 mirrors its own function's internal gate (io near-zero in BOTH
    // directions). S5/S7 now run a PER-APP census: the sorted walked-rel list must be built FIRST so the
    // app-root set (`app_roots`) and the per-app keyed-http counts (`keyed_by_root`) agree on the same
    // package.json set and bucket membership the census will use. The `all_walked_rels` build/sort and
    // the two pure map passes are cheap (no disk IO); the census's file reads stay guarded behind
    // `census_gate`, so a healthy single-package tree still does no IO.
    let io_provides_count = io_provides.len();
    let io_consumes_keyed_count = io_consumes.iter().filter(|c| c.key.is_some()).count();
    let s3_gate = io_provides_count < crate::framework_silence::IO_NEAR_ZERO_FLOOR
        && io_consumes_keyed_count < crate::framework_silence::IO_NEAR_ZERO_FLOOR;

    let mut all_walked_rels: Vec<String> = loc_by_path.keys().cloned().collect();
    all_walked_rels.sort();
    // Per-app bucketing: app-root dirs (parent of each package.json, plus the always-present `""` root
    // remainder) and the per-root count of KEYED `http` consumes. Deliberately KEYED-only — narrower
    // than S4's all-records count, per `builtin_fetch_lexical_warning`'s own doc (fetch is a recognized
    // extraction shape, so unresolved records would silence the tripwire on the join-blind trees it
    // targets). A single-package tree collapses to `[""]`, so `keyed_by_root[""]` reduces the gate below
    // to the exact pre-per-app tree-wide `keyed < MIN_PROVIDES_FLOOR` behavior.
    let roots = crate::framework_silence::app_roots(&all_walked_rels);
    let keyed_by_root = crate::framework_silence::keyed_http_by_root(io_consumes, &roots);
    // ANY below-floor app-root bucket can carry a dark app (a healthy sibling no longer masks it) — so
    // the census must run if any bucket is below floor. Single-package => identical to the old
    // tree-wide `keyed < MIN_PROVIDES_FLOOR` gate.
    let census_gate = keyed_by_root
        .values()
        .any(|&k| k < crate::framework_silence::MIN_PROVIDES_FLOOR);

    if s3_gate || census_gate {
        // S3 — committed-spec io-silence tripwire (consume side): a committed OpenAPI/Swagger spec
        // present while this tree's io stays near-zero in BOTH directions (the generated-client
        // blind spot).
        if s3_gate {
            if let Some(w) = crate::framework_silence::committed_spec_io_silence_warning(
                root,
                &all_walked_rels,
                io_provides_count,
                io_consumes_keyed_count,
            ) {
                warnings.push(w);
            }
        }
        // S5 — builtin-fetch internal-intent census (consume side), PER-APP: many lexical internal
        // `fetch(` call sites within an app whose keyed http consumes stay near-zero. May push multiple
        // per-app entries + an optional tree-wide fallback. Additive to S1-S4 above.
        if census_gate {
            let before = warnings.len();
            warnings.extend(crate::framework_silence::builtin_fetch_census(
                root,
                &all_walked_rels,
                &keyed_by_root,
                &roots,
            ));
            // S7 — fetch-wrapper call-site census (consume side), PER-APP: the wrapper-indirection dual
            // of S5, sharing S5's exact per-app gate. Additive to S1-S6 above.
            warnings.extend(crate::framework_silence::fetch_wrapper_census(
                root,
                &all_walked_rels,
                &keyed_by_root,
                &roots,
                wrapper_export_names,
            ));
            // Both censuses are consume-side alarms like S4 — counted by whether they PUSHED, since
            // each may emit any number of per-app entries (or none).
            consume_side_alarm |= warnings.len() > before;
        }
    }

    // S15 — empty-channel consequence, appended last (see this function's doc). Two independent
    // gates, one per channel: a sibling must have raised the alarm AND the channel must be EXACTLY
    // empty. The siblings fire at NEAR-zero (`MIN_PROVIDES_FLOOR`), and "these rules can produce no
    // finding" is only true at zero — a tree that kept two extracted routes still has two for them to
    // judge, so quoting the measurement there would be the same half-truth in the other direction.
    for (alarm, empty, channel) in [
        (
            provide_side_alarm,
            http_count == 0,
            zzop_core::rule_channels::reads::HTTP_PROVIDES,
        ),
        (
            consume_side_alarm,
            http_consumes_count == 0,
            zzop_core::rule_channels::reads::HTTP_CONSUMES,
        ),
    ] {
        if alarm && empty {
            warnings.extend(crate::framework_silence::channel_consequence_warning(
                channel, rule_gate,
            ));
        }
    }

    warnings
}
