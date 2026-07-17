//! Phase 5: BE-framework coverage self-report (`crate::framework_silence`'s seven tripwires,
//! S1-S7) — flags a tree that LOOKS like it carries a framework surface zzop cannot see. Split out of
//! `super::assemble` as its own phase since all seven tripwires share the same `io_provides`/
//! `io_consumes`/`ts_paths`/`java_rels`/`package_import_files` inputs and are otherwise independent of
//! every other `assemble` phase.

use std::collections::BTreeMap;

/// Runs all seven framework-silence tripwires (S1-S7) and returns every warning that fired, in push
/// order S1/S2/S4/S6/S3/S5/S7 (S6 slotted after S4 at introduction; S3/S5 keep their pre-split tail
/// positions; S7 slotted after S5 at introduction, sharing S5's precheck block) — order matters for
/// `AnalyzeOutput::warnings`' documented stability, not correctness (each tripwire is independent).
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

    // S3/S5/S7 prechecks — each mirrors its own function's internal gate (S3: io near-zero in BOTH
    // directions; S5 and S7 share one gate: keyed http consumes near-zero), done here too so the
    // sorted-walked-rel-list build below (`loc_by_path.keys()` — same source as `file_count`, per that
    // field's own doc) is skipped entirely when neither precheck can pass.
    let io_provides_count = io_provides.len();
    let io_consumes_keyed_count = io_consumes.iter().filter(|c| c.key.is_some()).count();
    let s3_gate = io_provides_count < crate::framework_silence::IO_NEAR_ZERO_FLOOR
        && io_consumes_keyed_count < crate::framework_silence::IO_NEAR_ZERO_FLOOR;
    // S5/S7 gate substrate: KEYED `http` consumes only — deliberately narrower than S4's all-records
    // count, per `builtin_fetch_lexical_warning`'s own doc (fetch is a recognized extraction shape,
    // so unresolved records would silence the tripwire on exactly the join-blind trees it targets). S7
    // (`fetch_wrapper_call_site_warning`) reuses this identical gate rather than computing its own —
    // it targets the same join-blind shape S5 does (near-zero keyed http consumes), just via a
    // wrapper-indirection census instead of a tree-wide token count; see that function's own doc.
    let http_consumes_keyed_count = io_consumes
        .iter()
        .filter(|c| c.kind == "http" && c.key.is_some())
        .count();
    let s5_gate = http_consumes_keyed_count < crate::framework_silence::MIN_PROVIDES_FLOOR;
    // The sorted walked-rel list every prechecked tripwire below needs — built at most once, and not
    // at all on a tree with healthy io (the "cheap on the success path" convention, extended past
    // disk IO to the rel-list sort itself — see IO_NEAR_ZERO_FLOOR's doc).
    if s3_gate || s5_gate {
        let mut all_walked_rels: Vec<String> = loc_by_path.keys().cloned().collect();
        all_walked_rels.sort();
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
        // S5 — builtin-fetch lexical census (consume side): many lexical `fetch(` call tokens while
        // KEYED http consumes stay near-zero — the no-import gap S4 structurally cannot cover
        // (builtin fetch is a global, not a module specifier). Additive to S1-S4 above.
        if s5_gate {
            if let Some(w) = crate::framework_silence::builtin_fetch_lexical_warning(
                root,
                &all_walked_rels,
                http_consumes_keyed_count,
            ) {
                warnings.push(w);
            }
            // S7 — fetch-wrapper call-site census (consume side): the wrapper-indirection dual of S5,
            // sharing S5's exact gate (a tree that funnels all its egress through one hand-rolled
            // wrapper module still shows only ONE literal `fetch(` token tree-wide, so it needs the
            // same near-zero keyed-consumes precheck, not a separate one). Additive to S1-S6 above.
            if let Some(w) = crate::framework_silence::fetch_wrapper_call_site_warning(
                root,
                &all_walked_rels,
                http_consumes_keyed_count,
            ) {
                warnings.push(w);
            }
        }
    }

    warnings
}
