//! Phase 5: BE-framework coverage self-report (`crate::framework_silence`'s seven tripwires,
//! S1-S7) — flags a tree that LOOKS like it carries a framework surface zzop cannot see. Split out of
//! `super::assemble` as its own phase since all seven tripwires share the same `io_provides`/
//! `io_consumes`/`ts_paths`/`java_rels`/`package_import_files` inputs and are otherwise independent of
//! every other `assemble` phase.

use std::collections::BTreeMap;

/// Runs all seven framework-silence tripwires (S1-S7) and returns every warning that fired, in push
/// order S1/S2/S4/S6/S3/S5/S7 (S6 slotted after S4 at introduction; S3/S5 keep their pre-split tail
/// positions; S7 slotted after S5 at introduction, sharing S5's precheck block) — order matters for
/// `AnalyzeOutput::warnings`' documented stability, not correctness (each tripwire is independent). S5
/// and S7 are now per-app censuses: each may contribute MULTIPLE entries (one per below-floor app-root,
/// in sorted `app_roots` order) plus an optional tree-wide fallback, all of S5's before all of S7's.
pub(super) fn framework_silence_warnings(
    root: &std::path::Path,
    io_provides: &[zzop_core::IoProvide],
    io_consumes: &[zzop_core::IoConsume],
    ts_paths: &std::collections::HashSet<String>,
    java_rels: &[String],
    package_import_files: &BTreeMap<String, std::collections::BTreeSet<String>>,
    loc_by_path: &std::collections::HashMap<String, u32>,
) -> Vec<String> {
    let mut warnings = Vec::new();

    // BE-framework coverage self-report (`crate::framework_silence`): flags a tree that looks like it
    // has a backend but produced zero `http` provides — an unsupported/unrecognized framework signal
    // (S1). Computed here, while `io_provides`/`io_consumes`/`ts_paths`/`java_rels`/`package_import_files`
    // are still in scope.
    let http_count = io_provides.iter().filter(|p| p.kind == "http").count();
    let mut candidate_rels: Vec<String> = ts_paths.iter().cloned().collect();
    candidate_rels.extend(java_rels.iter().cloned());
    candidate_rels.sort();
    candidate_rels.dedup();
    if let Some(w) =
        crate::framework_silence::controller_silence_warning(root, &candidate_rels, http_count)
    {
        warnings.push(w);
    }

    // S2 — server-framework import tripwire (provide side): a server-framework package import present
    // while extracted `http` provides stay near-zero (closes the method-call registration idiom S1's
    // decorator regex cannot see). Additive to S1 above; both may fire. Pure map lookup over
    // `package_import_files` (already a sorted `BTreeMap`/`BTreeSet`) — no disk IO, so unconditional.
    if let Some(w) =
        crate::framework_silence::server_framework_import_warning(package_import_files, http_count)
    {
        warnings.push(w);
    }

    // S4 — http-client import tripwire (consume side): an http-CLIENT package import present while
    // extracted `http` consumes stay near-zero — the consume-side dual of S2. Additive to S1-S3 above;
    // any subset may fire together. `http_consumes_count` counts ALL extracted `http`-kind consume
    // records — keyed AND unresolved — per `client_library_import_warning`'s own doc on why. Pure map
    // lookup over `package_import_files`, no disk IO, so unconditional.
    let http_consumes_count = io_consumes.iter().filter(|c| c.kind == "http").count();
    if let Some(w) = crate::framework_silence::client_library_import_warning(
        package_import_files,
        http_consumes_count,
    ) {
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
            ));
        }
    }

    warnings
}
