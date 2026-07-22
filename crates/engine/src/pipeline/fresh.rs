//! Fresh (non-cached) artifact computation for one file.

use zzop_core::{ImportMap, IoFacts, RulePackDef};

use crate::dispatch::Language;
use crate::EngineConfig;

use super::findings::{eval_packs, schema_findings, schema_findings_eligible};
use super::parsers::{
    lexical_loc, parse_csharp, parse_go, parse_java21, parse_prisma, parse_python, parse_rust,
    parse_typescript,
};
use super::FileArtifact;

/// TS-only per-file extractor gate: runs `f` on a non-degraded TypeScript file, else an empty vec.
fn ts_only<T>(is_ts_fresh: bool, rel: &str, text: &str, f: fn(&str, &str) -> Vec<T>) -> Vec<T> {
    if is_ts_fresh {
        f(rel, text)
    } else {
        Vec::new()
    }
}

/// The "no cache entry available" path: size-cap / dispatch / parse / IO projection / per-file DSL rules — shared by the cache-miss path and (via `cache: None`) the cache-off path.
pub(super) fn compute_fresh_artifact(
    rel: &str,
    bytes: &[u8],
    text: &str,
    language: Option<Language>,
    config: &EngineConfig,
    packs: &[&RulePackDef],
) -> FileArtifact {
    if bytes.len() > config.size_cap {
        // Oversized: loc counted lexically, no symbols/imports/io, but the text is still scanned by line-scan DSL rules
        // (lexical-only files are excluded from structural projection, not rule evaluation). `store_bound_models`/`field_usage_tokens` are raw-text regex scans, never an AST parse, so they run here too (like the removed `scan_store_map`/`scan_field_usage` walks), unaffected by the size cap.
        let loc = lexical_loc(text);
        let (findings, rule_timings, minified_or_generated) =
            eval_packs(packs, rel, text, &[], None, &[], config.profile_rules);
        return FileArtifact {
            rel: rel.to_string(),
            symbols: Vec::new(),
            imports: ts_slot(language),
            re_exports: Vec::new(),
            dynamic_imports: Vec::new(),
            asset_refs: Vec::new(),
            loc,
            findings,
            degraded: true,
            minified_or_generated,
            io: None,
            rule_timings,
            used_names: Vec::new(),
            const_map_fragment: std::collections::HashMap::new(),
            procedure_router_fragments: Vec::new(),
            router_mount_fragments: Vec::new(),
            wrapper_def_fragments: Vec::new(),
            wrapper_call_fragments: Vec::new(),
            controller_prefix_route_fragments: Vec::new(),
            class_shape_fragments: Vec::new(),
            query_call_sites: Vec::new(),
            loop_spans: Vec::new(),
            field_usage_tokens: sorted_field_usage_tokens(rel, text),
        };
    }

    let (symbols, imports, loc, degraded, used_names) = match language {
        Some(Language::TypeScript) => parse_typescript(rel, text),
        Some(Language::Prisma) => {
            let (symbols, imports, loc, degraded) = parse_prisma(&config.source_id, rel, text);
            (symbols, imports, loc, degraded, Vec::new())
        }
        Some(Language::Java21) => parse_java21(rel, text),
        Some(Language::Python) => parse_python(rel, text),
        Some(Language::Rust) => parse_rust(rel, text),
        Some(Language::Go) => parse_go(rel, text),
        Some(Language::Sql) => (Vec::new(), None, lexical_loc(text), false, Vec::new()),
        Some(Language::CSharp) => parse_csharp(rel, text),
        None => (Vec::new(), None, lexical_loc(text), false, Vec::new()),
    };
    // IO projection. Egress consumes (TS/Python/Rust/Go/C#) run only on a well-formed, in-size-cap file. Route provides come
    // io-direct for TS (Hono), Java (Spring), and C# (attribute controllers + minimal APIs); Java's + C#'s provides project for ANY
    // `.java`/`.cs` file regardless of `degraded` (extractors return empty rather than guess — C# gates only its consumes). Python/Rust/Go ROUTE provides travel as `router_mount_fragments`; Python (SQLModel) & Go (GORM) ALSO emit io-direct `db-table` provides too.
    let io = match language {
        Some(Language::TypeScript) if !degraded => {
            crate::io::extract_file_io(rel, text, &config.io)
        }
        Some(Language::Java21) => crate::io::extract_java_file_io(rel, text),
        Some(Language::Python) if !degraded => {
            let mut consumes = zzop_parser_python_3::extract_python_http_consumes(rel, text);
            // ORM db-table facts (keyed engine-side): SQLModel/SQLAlchemy + Django — touches -> consumes, models -> provides.
            consumes.extend(zzop_parser_python_3::extract_sqlalchemy_db_table_consumes(
                rel, text,
            ));
            consumes.extend(zzop_parser_python_3::extract_django_db_table_consumes(
                rel, text,
            ));
            let mut provides =
                zzop_parser_python_3::extract_sqlalchemy_db_table_provides(rel, text);
            provides.extend(zzop_parser_python_3::extract_django_db_table_provides(
                rel, text,
            ));
            if consumes.is_empty() && provides.is_empty() {
                None
            } else {
                Some(IoFacts { provides, consumes })
            }
        }
        Some(Language::Rust) if !degraded => {
            let consumes = zzop_parser_rust::extract_rust_http_consumes(rel, text);
            if consumes.is_empty() {
                None
            } else {
                Some(IoFacts {
                    provides: Vec::new(),
                    consumes,
                })
            }
        }
        Some(Language::Go) if !degraded => {
            let mut consumes = zzop_parser_go::extract_go_http_consumes(rel, text);
            // GORM db-table facts: model touches -> consumes (keyed engine-side), `gorm.Model` structs -> provides.
            consumes.extend(zzop_parser_go::extract_gorm_db_table_consumes(rel, text));
            let provides = zzop_parser_go::extract_gorm_db_table_provides(rel, text);
            if consumes.is_empty() && provides.is_empty() {
                None
            } else {
                Some(IoFacts { provides, consumes })
            }
        }
        Some(Language::Sql) => {
            let provides = zzop_parser_sql::extract_db_table_provides(rel, text);
            (!provides.is_empty()).then(|| IoFacts {
                provides,
                consumes: Vec::new(),
            })
        }
        // C# projects BOTH provides (attribute controllers + minimal APIs, io-direct — no
        // `router_mount_fragments` arm) and `HttpClient` egress consumes. Unlike the egress-only arms
        // above, this is NOT gated `if !degraded`: `extract_csharp_file_io` runs its route-PROVIDES side
        // unconditionally (Java-parity), gating only the consumes internally — see its own doc.
        Some(Language::CSharp) => crate::io::extract_csharp_file_io(rel, text, degraded),
        _ => None,
    };
    // The next projections are all TypeScript-only, reusing `text` already in hand (no second file read): const-map fragment (feeds
    // `analyze::assemble`'s merge + late consume re-resolution), tRPC router fragment (`analyze::compose_trpc_provides`), router-mount
    // fragment (Hono builders/cross-file mounts, for `analyze::compose_router_mount_provides`), wrapper def/call fragments (assemble-time wrapper-consume join, defs indexed by `(file, name)`), controller-prefix route fragment (assemble-time controller-prefix composer, against the same const map), and query-call-site facts (`run_schema_join_rules` substrate).
    let const_map_fragment = match language {
        Some(Language::TypeScript) if !degraded => {
            zzop_parser_typescript::const_map_fragment(rel, text)
        }
        _ => std::collections::HashMap::new(),
    };
    let procedure_router_fragments = match language {
        Some(Language::TypeScript) if !degraded => {
            zzop_parser_typescript::extract_procedure_router_fragments(rel, text)
        }
        _ => Vec::new(),
    };
    let router_mount_fragments = match language {
        Some(Language::TypeScript) if !degraded => {
            let router_names: Vec<&str> =
                config.io.router_names.iter().map(String::as_str).collect();
            zzop_parser_typescript::extract_router_mount_fragments(rel, text, &router_names)
        }
        // FastAPI receivers AND Django `urlpatterns` project the SAME router-mount-fragment shape (`adapters::fastapi`/`::django_routes`), both composed by the identical `compose_router_mount_provides` pass below — merged here (the two never coexist in one Python file).
        Some(Language::Python) if !degraded => {
            let mut f = zzop_parser_python_3::extract_fastapi_router_fragments(rel, text);
            f.extend(zzop_parser_python_3::extract_django_route_fragments(
                rel, text,
            ));
            f
        }
        // axum router builders project into the SAME framework-neutral router-mount-fragment shape — see `zzop_parser_rust::adapters::axum`'s module doc. Composed by the identical `analyze::compose_router_mount_provides` pass below, no separate Rust-only composition path.
        Some(Language::Rust) if !degraded => {
            zzop_parser_rust::extract_axum_router_fragments(rel, text)
        }
        // gin route groups AND net/http mux registrations both project into the SAME framework-neutral router-mount-fragment shape —
        // see `zzop_parser_go::adapters`'s module doc. Composed by the identical `analyze::compose_router_mount_provides` pass below, no separate Go-only composition path.
        Some(Language::Go) if !degraded => zzop_parser_go::extract_go_router_fragments(rel, text),
        _ => Vec::new(),
    };
    // TS-only structural signals (empty for non-TS/degraded, see `ts_only`): re-exports + dynamic `import()` feed the dep graph (Defects A/2); asset-URL refs feed the fan-in bump — see field docs.
    let is_ts_fresh = matches!(language, Some(Language::TypeScript)) && !degraded;
    let re_exports = ts_only(
        is_ts_fresh,
        rel,
        text,
        zzop_parser_typescript::parse_re_exports,
    );
    let dynamic_imports = ts_only(
        is_ts_fresh,
        rel,
        text,
        zzop_parser_typescript::parse_dynamic_imports,
    );
    let asset_refs = ts_only(
        is_ts_fresh,
        rel,
        text,
        zzop_parser_typescript::parse_asset_refs,
    );
    let (wrapper_def_fragments, wrapper_call_fragments) = match language {
        Some(Language::TypeScript) if !degraded => {
            zzop_parser_typescript::extract_wrapper_fragments(rel, text)
        }
        _ => (Vec::new(), Vec::new()),
    };
    // Controller-prefix route fragment (`controller-prefix-ref-v1`): a `@Controller(RouteKey.Asset)` dotted member-expression prefix, deferred to `analyze`'s assemble-time controller-prefix composer (same merged const map `const_map_fragment` above feeds).
    let controller_prefix_route_fragments = match language {
        Some(Language::TypeScript) if !degraded => {
            zzop_parser_typescript::extract_controller_prefix_route_fragments(rel, text)
        }
        _ => Vec::new(),
    };
    // Class field-shape fragments (`body-shape-v1`): the DTO-resolution substrate for `IoProvide::body.dto_ref`, deferred to `analyze`'s assemble-time resolver (same fragment -> tree-wide-merge pattern as the controller-prefix composer above).
    let class_shape_fragments = match language {
        Some(Language::TypeScript) if !degraded => {
            zzop_parser_typescript::extract_class_shape_fragments(rel, text)
        }
        _ => Vec::new(),
    };
    let query_call_sites = match language {
        Some(Language::TypeScript) if !degraded => {
            zzop_parser_typescript::extract_query_call_sites(rel, text)
        }
        _ => Vec::new(),
    };
    // Loop-body line spans (`loop-spans-v1`): AST-derived, so it follows the `symbols`-style per-language/non-degraded gate above (TypeScript + Go today, never the `store_bound_models`/`field_usage_tokens` regex-scan gate below) — `MethodScan::trigger_in_loop`'s substrate.
    let loop_spans = match language {
        Some(Language::TypeScript) if !degraded => {
            zzop_parser_typescript::extract_loop_spans(rel, text)
        }
        Some(Language::Go) if !degraded => zzop_parser_go::extract_loop_spans(rel, text),
        _ => Vec::new(),
    };
    // Store-binding and field-usage-token facts are both raw-text regex scans, never an AST parse, so — like the removed `scan_store_map`/`scan_field_usage` filesystem walks they replace — they run unconditionally on `rel`/`text` here regardless of `language`/`degraded`; each gates its own applicability internally (the store-file convention, the `.ts`/`.tsx` extension, respectively).
    let field_usage_tokens = sorted_field_usage_tokens(rel, text);
    let (mut findings, rule_timings, minified_or_generated) = eval_packs(
        packs,
        rel,
        text,
        &symbols,
        io.clone(),
        &loop_spans,
        config.profile_rules,
    );
    if schema_findings_eligible(language, degraded) {
        findings.extend(schema_findings(&config.rule_config, rel, text));
    }
    FileArtifact {
        rel: rel.to_string(),
        symbols,
        imports,
        re_exports,
        dynamic_imports,
        asset_refs,
        loc,
        findings,
        degraded,
        minified_or_generated,
        io,
        rule_timings,
        used_names,
        const_map_fragment,
        procedure_router_fragments,
        router_mount_fragments,
        wrapper_def_fragments,
        wrapper_call_fragments,
        controller_prefix_route_fragments,
        class_shape_fragments,
        query_call_sites,
        field_usage_tokens,
        loop_spans,
    }
}

/// `zzop_rules_schema::field_usage_tokens`'s presence-only result, sorted for deterministic serialization — mirrors `used_names`'s own "sorted" convention on `FileArtifact`/`FileIrSlice`.
fn sorted_field_usage_tokens(rel: &str, text: &str) -> Vec<String> {
    let mut tokens: Vec<String> = zzop_rules_schema::field_usage_tokens(rel, text)
        .into_iter()
        .collect();
    tokens.sort();
    tokens
}

/// `Some(empty map)` for a TypeScript-, Python-, Rust-, Go-, or Java21-dispatched file (gives it a dep-graph node even when parsing
/// was skipped/degraded), `None` otherwise. Named `ts_slot` for historical reasons (predates Python/Rust/Go/Java21 dispatch) — see
/// `FileArtifact::imports`'s doc for what participating in this slot actually grants downstream. `.java` joined this slot only once its dispatch target became a real structural parser (`Language::Java21`) — the retired lexical brace-matcher never produced an `ImportMap` at all, so `.java` was excluded here before.
fn ts_slot(language: Option<Language>) -> Option<ImportMap> {
    matches!(
        language,
        Some(Language::TypeScript)
            | Some(Language::Python)
            | Some(Language::Rust)
            | Some(Language::Go)
            | Some(Language::Java21)
            | Some(Language::CSharp)
    )
    .then(ImportMap::new)
}
