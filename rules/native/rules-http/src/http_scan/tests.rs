//! Tests for `scan_unsafe_read_endpoint` and `scan_non_idempotent_write`. Fixtures build real
//! `write_sites` via `zzop_parser_typescript::write_sites_for_symbol` (the same function production
//! code calls at parse time) rather than re-implementing a test double, so these tests exercise the
//! real detection + the BFS/selection logic together. Every fixture body is single-line, so
//! `body_start == body_end == <declaration line>`.
use super::*;
use zzop_core::callgraph::SymbolEdge;
use zzop_core::{ApiEndpoint, SourceSymbolKind};

fn files(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(f, t)| (f.to_string(), t.to_string()))
        .collect()
}

fn sym(file: &str, name: &str, line: u32) -> SourceSymbol {
    SourceSymbol {
        id: format!("{file}#{name}"),
        file: file.to_string(),
        name: name.to_string(),
        kind: SourceSymbolKind::Function,
        line,
        exported: true,
        is_default: false,
        body_start: Some(line),
        body_end: Some(line),
        write_sites: Vec::new(),
    }
}

/// Fills in each symbol's `write_sites` from its own file's text, using the moved detection function —
/// mirrors what `zzop_parser_typescript::parse_symbols` does for a real TS parse.
fn with_write_sites(
    files: &HashMap<String, String>,
    symbols: Vec<SourceSymbol>,
) -> Vec<SourceSymbol> {
    symbols
        .into_iter()
        .map(|mut s| {
            if let Some(text) = files.get(&s.file) {
                s.write_sites = zzop_parser_typescript::write_sites_for_symbol(&s, text);
            }
            s
        })
        .collect()
}

fn endpoint(method: &str, path: &str, handler: &str) -> ApiEndpoint {
    ApiEndpoint {
        method: method.to_string(),
        path: path.to_string(),
        handler: handler.to_string(),
    }
}

fn edge(from: &str, to: &str) -> SymbolEdge {
    SymbolEdge {
        from: from.to_string(),
        to: to.to_string(),
    }
}

// --- scan_unsafe_read_endpoint ---

#[test]
fn get_handler_reaching_a_write_across_a_call_edge_is_flagged_with_hops() {
    let files = files(&[
        (
            "api/handlers.ts",
            "export function activateUser(c: any) { return service.activate(c.id); }\nexport function getUser(c: any) { return userStore.findUnique({ where: { id: c.id } }); }\n",
        ),
        (
            "api/service.ts",
            "export function activate(id: string) { return prisma.user.update({ where: { id }, data: { active: true } }); }\n",
        ),
    ]);
    let symbols = with_write_sites(
        &files,
        vec![
            sym("api/handlers.ts", "activateUser", 1),
            sym("api/handlers.ts", "getUser", 2),
            sym("api/service.ts", "activate", 1),
        ],
    );
    let graph = vec![edge(
        "api/handlers.ts#activateUser",
        "api/service.ts#activate",
    )];
    let endpoints = vec![
        endpoint("GET", "/users/:id/activate", "activateUser"),
        endpoint("GET", "/users/:id", "getUser"),
    ];
    let out = scan_unsafe_read_endpoint(&ScanUnsafeReadEndpointInput {
        api_endpoints: &endpoints,
        symbols: &symbols,
        symbol_graph: &graph,
        files: &files,
    });
    assert_eq!(out.len(), 1);
    let data = out[0].data.as_ref().unwrap();
    assert_eq!(data["method"], "GET");
    assert_eq!(data["path"], "/users/:id/activate");
    assert_eq!(data["sink"], "prisma.user.update");
    assert_eq!(data["writeFile"], "api/service.ts");
    assert_eq!(data["depth"], 1);
}

#[test]
fn write_directly_in_the_handler_is_depth_zero() {
    let files = files(&[(
        "api/h.ts",
        "export function touch(c: any) { return prisma.ping.create({ data: {} }); }\n",
    )]);
    let symbols = with_write_sites(&files, vec![sym("api/h.ts", "touch", 1)]);
    let out = scan_unsafe_read_endpoint(&ScanUnsafeReadEndpointInput {
        api_endpoints: &[endpoint("GET", "/touch", "touch")],
        symbols: &symbols,
        symbol_graph: &Vec::new(),
        files: &files,
    });
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].data.as_ref().unwrap()["depth"], 0);
}

/// Pins the exact rendered message — regression coverage for the mid-sentence, lowercase-"disable"
/// `disable_hint` splice this message went through during the 2026-07-10 dialect-consolidation sweep
/// (unlike most native messages, this one reads "...or disable {tail}", not "...Disable via config...").
#[test]
fn unsafe_read_endpoint_message_is_byte_identical_to_the_pre_sweep_text() {
    let files = files(&[(
        "api/h.ts",
        "export function touch(c: any) { return prisma.ping.create({ data: {} }); }\n",
    )]);
    let symbols = with_write_sites(&files, vec![sym("api/h.ts", "touch", 1)]);
    let out = scan_unsafe_read_endpoint(&ScanUnsafeReadEndpointInput {
        api_endpoints: &[endpoint("GET", "/touch", "touch")],
        symbols: &symbols,
        symbol_graph: &Vec::new(),
        files: &files,
    });
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].rule_id, "unsafe-read-endpoint");
    assert_eq!(
        out[0].message,
        "GET /touch writes directly (prisma.ping.create) — GET/HEAD must be safe & idempotent. Move \
         the write behind a mutating method (POST/PUT/PATCH/DELETE), or make this endpoint genuinely \
         read-only. If the write is deliberate and safe to repeat (e.g. a fire-and-forget audit log), \
         mark it with `// idempotent-ok: <reason>` on the line above the handler, or disable via \
         config `rules: { \"unsafe-read-endpoint\": \"off\" }` (embedders: `disabled_rules`) if this \
         applies more broadly."
    );
}

#[test]
fn non_safe_methods_are_never_flagged_even_when_they_write() {
    let files = files(&[(
        "api/h.ts",
        "export function create(c: any) { return prisma.user.create({ data: {} }); }\n",
    )]);
    let symbols = with_write_sites(&files, vec![sym("api/h.ts", "create", 1)]);
    let out = scan_unsafe_read_endpoint(&ScanUnsafeReadEndpointInput {
        api_endpoints: &[endpoint("POST", "/users", "create")],
        symbols: &symbols,
        symbol_graph: &Vec::new(),
        files: &files,
    });
    assert!(out.is_empty());
}

#[test]
fn read_only_get_handler_has_no_finding() {
    let files = files(&[(
        "api/h.ts",
        "export function list(c: any) { return prisma.user.findMany(); }\n",
    )]);
    let symbols = with_write_sites(&files, vec![sym("api/h.ts", "list", 1)]);
    let out = scan_unsafe_read_endpoint(&ScanUnsafeReadEndpointInput {
        api_endpoints: &[endpoint("GET", "/users", "list")],
        symbols: &symbols,
        symbol_graph: &Vec::new(),
        files: &files,
    });
    assert!(out.is_empty());
}

#[test]
fn get_reaching_a_raw_sql_write_across_an_edge_is_flagged() {
    let files = files(&[
        ("api/h.ts", "export function getRates(c: any) { return refresh(c.env); }\n"),
        (
            "api/refresh.ts",
            "export async function refresh(env: any) { await env.DB.prepare(\"INSERT INTO fx_rates (id, rates) VALUES (1, ?)\").bind(x).run(); }\n",
        ),
    ]);
    let symbols = with_write_sites(
        &files,
        vec![
            sym("api/h.ts", "getRates", 1),
            sym("api/refresh.ts", "refresh", 1),
        ],
    );
    let graph = vec![edge("api/h.ts#getRates", "api/refresh.ts#refresh")];
    let out = scan_unsafe_read_endpoint(&ScanUnsafeReadEndpointInput {
        api_endpoints: &[endpoint("GET", "/api/rates", "getRates")],
        symbols: &symbols,
        symbol_graph: &graph,
        files: &files,
    });
    assert_eq!(out.len(), 1);
    let data = out[0].data.as_ref().unwrap();
    assert!(data["sink"]
        .as_str()
        .unwrap()
        .contains("INSERT INTO fx_rates"));
    assert_eq!(data["depth"], 1);
}

#[test]
fn get_that_only_runs_a_select_is_not_flagged() {
    let files = files(&[(
        "api/h.ts",
        "export function list(c: any) { return c.env.DB.prepare(\"SELECT * FROM fx_rates\").all(); }\n",
    )]);
    let symbols = with_write_sites(&files, vec![sym("api/h.ts", "list", 1)]);
    let out = scan_unsafe_read_endpoint(&ScanUnsafeReadEndpointInput {
        api_endpoints: &[endpoint("GET", "/api/rates", "list")],
        symbols: &symbols,
        symbol_graph: &Vec::new(),
        files: &files,
    });
    assert!(out.is_empty());
}

#[test]
fn idempotent_ok_marker_above_the_handler_suppresses_the_finding() {
    let files = files(&[(
        "api/h.ts",
        "// idempotent-ok: write is a fire-and-forget audit log, safe to repeat\nexport function touch(c: any) { return prisma.ping.create({ data: {} }); }\n",
    )]);
    let symbols = with_write_sites(&files, vec![sym("api/h.ts", "touch", 2)]);
    let out = scan_unsafe_read_endpoint(&ScanUnsafeReadEndpointInput {
        api_endpoints: &[endpoint("GET", "/touch", "touch")],
        symbols: &symbols,
        symbol_graph: &Vec::new(),
        files: &files,
    });
    assert!(out.is_empty());
}

#[test]
fn ambiguous_handler_name_defined_in_two_files_is_skipped() {
    let files = files(&[
        (
            "api/a.ts",
            "export function dup(c: any) { return prisma.user.create({ data: {} }); }\n",
        ),
        ("api/b.ts", "export function dup(c: any) { return 1; }\n"),
    ]);
    let symbols = with_write_sites(
        &files,
        vec![sym("api/a.ts", "dup", 1), sym("api/b.ts", "dup", 1)],
    );
    let out = scan_unsafe_read_endpoint(&ScanUnsafeReadEndpointInput {
        api_endpoints: &[endpoint("GET", "/x", "dup")],
        symbols: &symbols,
        symbol_graph: &Vec::new(),
        files: &files,
    });
    assert!(out.is_empty());
}

#[test]
fn wrapped_handler_resolves_to_the_inner_identifier() {
    let files = files(&[(
        "api/h.ts",
        "export function getThing(c: any) { return prisma.thing.delete({ where: { id: 1 } }); }\n",
    )]);
    let symbols = with_write_sites(&files, vec![sym("api/h.ts", "getThing", 1)]);
    let out = scan_unsafe_read_endpoint(&ScanUnsafeReadEndpointInput {
        api_endpoints: &[endpoint("GET", "/thing", "rateLimit(getThing)")],
        symbols: &symbols,
        symbol_graph: &Vec::new(),
        files: &files,
    });
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].data.as_ref().unwrap()["sink"], "prisma.thing.delete");
}

// --- scan_non_idempotent_write ---

#[test]
fn put_handler_that_creates_a_row_is_flagged_kind_create() {
    let files = files(&[("api/h.ts", "export function putThing(c: any) { return prisma.thing.create({ data: { id: c.id } }); }\n")]);
    let symbols = with_write_sites(&files, vec![sym("api/h.ts", "putThing", 1)]);
    let out = scan_non_idempotent_write(&ScanNonIdempotentWriteInput {
        api_endpoints: &[endpoint("PUT", "/things/:id", "putThing")],
        symbols: &symbols,
        symbol_graph: &Vec::new(),
        files: &files,
    });
    assert_eq!(out.len(), 1);
    let data = out[0].data.as_ref().unwrap();
    assert_eq!(data["method"], "PUT");
    assert_eq!(data["kind"], "create");
    assert_eq!(data["sink"], "prisma.thing.create");
    assert_eq!(data["depth"], 0);
}

/// Pins the exact rendered message — regression coverage for the `disable_hint` splice
/// `hint_for`/`scan_non_idempotent_write` went through during the 2026-07-10 dialect-consolidation sweep.
#[test]
fn non_idempotent_write_message_is_byte_identical_to_the_pre_sweep_text() {
    let files = files(&[("api/h.ts", "export function putThing(c: any) { return prisma.thing.create({ data: { id: c.id } }); }\n")]);
    let symbols = with_write_sites(&files, vec![sym("api/h.ts", "putThing", 1)]);
    let out = scan_non_idempotent_write(&ScanNonIdempotentWriteInput {
        api_endpoints: &[endpoint("PUT", "/things/:id", "putThing")],
        symbols: &symbols,
        symbol_graph: &Vec::new(),
        files: &files,
    });
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].rule_id, "non-idempotent-write");
    assert_eq!(
        out[0].message,
        "PUT /things/:id reaches prisma.thing.create directly (create) — a retry inserts a duplicate \
         row; PUT must be idempotent. Add an idempotency key or a dedup/uniqueness check before the \
         write, or mark it with `// idempotent-ok: <reason>` above the handler if a retry is \
         genuinely safe here. Disable via config `rules: { \"non-idempotent-write\": \"off\" }` \
         (embedders: `disabled_rules`) if this applies more broadly."
    );
}

#[test]
fn delete_reaching_a_create_across_a_call_edge_is_flagged_with_hops() {
    let files = files(&[
        ("api/h.ts", "export function removeThing(c: any) { return audit.log(c.id); }\n"),
        (
            "api/audit.ts",
            "export function log(id: string) { return prisma.auditRow.create({ data: { id } }); }\n",
        ),
    ]);
    let symbols = with_write_sites(
        &files,
        vec![
            sym("api/h.ts", "removeThing", 1),
            sym("api/audit.ts", "log", 1),
        ],
    );
    let graph = vec![edge("api/h.ts#removeThing", "api/audit.ts#log")];
    let out = scan_non_idempotent_write(&ScanNonIdempotentWriteInput {
        api_endpoints: &[endpoint("DELETE", "/things/:id", "removeThing")],
        symbols: &symbols,
        symbol_graph: &graph,
        files: &files,
    });
    assert_eq!(out.len(), 1);
    let data = out[0].data.as_ref().unwrap();
    assert_eq!(data["method"], "DELETE");
    assert_eq!(data["kind"], "create");
    assert_eq!(data["depth"], 1);
}

#[test]
fn put_with_atomic_increment_is_flagged_kind_atomic_accumulate() {
    let files = files(&[(
        "api/h.ts",
        "export function bump(c: any) { return prisma.counter.update({ where: { id: c.id }, data: { hits: { increment: 1 } } }); }\n",
    )]);
    let symbols = with_write_sites(&files, vec![sym("api/h.ts", "bump", 1)]);
    let out = scan_non_idempotent_write(&ScanNonIdempotentWriteInput {
        api_endpoints: &[endpoint("PUT", "/counter/:id", "bump")],
        symbols: &symbols,
        symbol_graph: &Vec::new(),
        files: &files,
    });
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].data.as_ref().unwrap()["kind"], "atomic-accumulate");
}

#[test]
fn put_with_a_plain_idempotent_update_is_not_flagged() {
    let files = files(&[(
        "api/h.ts",
        "export function setName(c: any) { return prisma.user.update({ where: { id: c.id }, data: { name: c.name } }); }\n",
    )]);
    let symbols = with_write_sites(&files, vec![sym("api/h.ts", "setName", 1)]);
    let out = scan_non_idempotent_write(&ScanNonIdempotentWriteInput {
        api_endpoints: &[endpoint("PUT", "/users/:id", "setName")],
        symbols: &symbols,
        symbol_graph: &Vec::new(),
        files: &files,
    });
    assert!(out.is_empty());
}

#[test]
fn put_using_upsert_is_not_flagged() {
    let files = files(&[(
        "api/h.ts",
        "export function put(c: any) { return prisma.profile.upsert({ where: { id: c.id }, create: { id: c.id }, update: { name: c.name } }); }\n",
    )]);
    let symbols = with_write_sites(&files, vec![sym("api/h.ts", "put", 1)]);
    let out = scan_non_idempotent_write(&ScanNonIdempotentWriteInput {
        api_endpoints: &[endpoint("PUT", "/profile/:id", "put")],
        symbols: &symbols,
        symbol_graph: &Vec::new(),
        files: &files,
    });
    assert!(out.is_empty());
}

#[test]
fn counter_bump_via_a_store_like_receiver_is_flagged_kind_counter() {
    let files = files(&[(
        "api/h.ts",
        "export function put(c: any) { return rateStore.incr(c.key); }\n",
    )]);
    let symbols = with_write_sites(&files, vec![sym("api/h.ts", "put", 1)]);
    let out = scan_non_idempotent_write(&ScanNonIdempotentWriteInput {
        api_endpoints: &[endpoint("PUT", "/rate/:key", "put")],
        symbols: &symbols,
        symbol_graph: &Vec::new(),
        files: &files,
    });
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].data.as_ref().unwrap()["kind"], "counter");
}

#[test]
fn post_and_get_with_a_bare_create_are_not_flagged() {
    let files = files(&[(
        "api/h.ts",
        "export function create(c: any) { return prisma.user.create({ data: {} }); }\n",
    )]);
    let symbols = with_write_sites(&files, vec![sym("api/h.ts", "create", 1)]);
    let out = scan_non_idempotent_write(&ScanNonIdempotentWriteInput {
        api_endpoints: &[
            endpoint("POST", "/users", "create"),
            endpoint("GET", "/users", "create"),
        ],
        symbols: &symbols,
        symbol_graph: &Vec::new(),
        files: &files,
    });
    assert!(out.is_empty());
}

#[test]
fn post_with_atomic_increment_is_flagged_regardless_of_method() {
    let files = files(&[(
        "api/h.ts",
        "export function vote(c: any) { return prisma.poll.update({ where: { id: c.id }, data: { votes: { increment: 1 } } }); }\n",
    )]);
    let symbols = with_write_sites(&files, vec![sym("api/h.ts", "vote", 1)]);
    let out = scan_non_idempotent_write(&ScanNonIdempotentWriteInput {
        api_endpoints: &[endpoint("POST", "/polls/:id/vote", "vote")],
        symbols: &symbols,
        symbol_graph: &Vec::new(),
        files: &files,
    });
    assert_eq!(out.len(), 1);
    let data = out[0].data.as_ref().unwrap();
    assert_eq!(data["method"], "POST");
    assert_eq!(data["kind"], "atomic-accumulate");
    assert!(data["hint"].as_str().unwrap().contains("idempotency key"));
}

#[test]
fn idempotent_ok_marker_suppresses_non_idempotent_write_finding() {
    let files = files(&[(
        "api/h.ts",
        "// idempotent-ok: create guarded by a unique constraint, retry is a no-op\nexport function put(c: any) { return prisma.thing.create({ data: { id: c.id } }); }\n",
    )]);
    let symbols = with_write_sites(&files, vec![sym("api/h.ts", "put", 2)]);
    let out = scan_non_idempotent_write(&ScanNonIdempotentWriteInput {
        api_endpoints: &[endpoint("PUT", "/things/:id", "put")],
        symbols: &symbols,
        symbol_graph: &Vec::new(),
        files: &files,
    });
    assert!(out.is_empty());
}
