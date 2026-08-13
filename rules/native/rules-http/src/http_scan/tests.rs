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
         mark it with `// idempotent-ok: <reason>` on the body-start line or up to 3 lines above, or disable via \
         config `rules: { \"unsafe-read-endpoint\": \"off\" }` (embedders: `disabledRules`) if this \
         applies more broadly. LANGUAGE SIGHTLINE: this check needs store-write evidence that only the \
         TypeScript parser produces (ts/tsx/js/jsx/mjs/cjs/mts/cts) — `SourceSymbol::write_sites`, \
         which parser-python-3/go/rust/csharp/java-21 all leave empty, so a handler in those languages \
         has no write site the call-graph BFS could reach and this rule cannot fire there at all. \
         Easiest to misread: the sibling `mutating-route-no-auth` rule DOES walk Java, so a Java repo \
         can show that rule's findings while this rule stayed dark on the very same routes. ZERO \
         findings of this rule outside ts/tsx/js/jsx/mjs/cjs/mts/cts therefore means NOT ANALYZED, \
         never \"no risky write on these routes\"."
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

// ---------------------------------------------------------------------------------------------
// `idempotent-ok` near-miss disclosure (see `with_ok_marker_near_miss`). The DSL interpreter got the
// same disclosure; this hand-authored native marker had none, so a misspelled marker failed silently.
// ---------------------------------------------------------------------------------------------

/// One GET endpoint whose handler writes, with `comment` on the line above the handler.
fn unsafe_read_with_comment(comment: &str) -> Vec<zzop_core::Finding> {
    let files = files(&[(
        "api/h.ts",
        &format!(
            "{comment}\nexport function touch(c: any) {{ return prisma.ping.create({{ data: {{}} }}); }}\n"
        ),
    )]);
    let symbols = with_write_sites(&files, vec![sym("api/h.ts", "touch", 2)]);
    scan_unsafe_read_endpoint(&ScanUnsafeReadEndpointInput {
        api_endpoints: &[endpoint("GET", "/touch", "touch")],
        symbols: &symbols,
        symbol_graph: &Vec::new(),
        files: &files,
    })
}

#[test]
fn a_misspelled_ok_marker_is_disclosed_instead_of_failing_silently() {
    for comment in [
        "// idempotent-okay:",
        "// non-idempotent-write-ok",
        "// unsafe-read-endpoint-ok: intentional",
    ] {
        let out = unsafe_read_with_comment(comment);
        assert_eq!(out.len(), 1, "{comment} must not suppress: {out:?}");
        let m = &out[0].message;
        assert!(m.contains("does not suppress this rule"), "{m}");
        assert!(
            m.contains("`// idempotent-ok: <reason>`"),
            "the honored spelling must be named: {m}"
        );
        assert!(m.contains("the trailing colon is required"), "{m}");
    }
}

#[test]
fn the_honored_marker_without_its_required_colon_is_disclosed() {
    // The native regex is `//\s*idempotent-ok:` — a bare `// idempotent-ok` does NOT suppress. The DSL
    // sibling honors both spellings, so this trap is unique to this surface and must be disclosed.
    let out = unsafe_read_with_comment("// idempotent-ok");
    assert_eq!(out.len(), 1, "{out:?}");
    assert!(
        out[0].message.contains("reads `idempotent-ok`"),
        "{}",
        out[0].message
    );
    assert!(out[0].message.contains("the trailing colon is required"));
}

#[test]
fn the_honored_marker_suppresses_and_never_accuses_itself() {
    // Negative control: the correct spelling suppresses, so the disclosure path is never reached.
    assert!(unsafe_read_with_comment("// idempotent-ok: audit log").is_empty());
}

#[test]
fn ordinary_prose_above_a_handler_is_never_accused() {
    // Mirrors the DSL near-miss shape's own prose guarantees: a `-ok` word inside a sentence, a
    // capitalized one, or one preceded by another word never matches the marker shape.
    for comment in [
        "// half-ok for now, revisit",
        "// NOT-ok:",
        "// TODO: not-ok",
        "// plain comment",
    ] {
        let out = unsafe_read_with_comment(comment);
        assert_eq!(out.len(), 1, "{comment}: {out:?}");
        assert!(
            !out[0].message.contains("does not suppress this rule"),
            "{comment} must not be accused: {}",
            out[0].message
        );
    }
}

#[test]
fn the_disclosure_also_reaches_non_idempotent_write_and_stays_in_sync_with_data_hint() {
    let files = files(&[(
        "api/h.ts",
        "// idempotent-okay: typo\nexport function put(c: any) { return prisma.thing.create({ data: { id: c.id } }); }\n",
    )]);
    let symbols = with_write_sites(&files, vec![sym("api/h.ts", "put", 2)]);
    let out = scan_non_idempotent_write(&ScanNonIdempotentWriteInput {
        api_endpoints: &[endpoint("PUT", "/thing", "put")],
        symbols: &symbols,
        symbol_graph: &Vec::new(),
        files: &files,
    });
    assert_eq!(out.len(), 1, "{out:?}");
    assert!(
        out[0].message.contains("reads `idempotent-okay`"),
        "{}",
        out[0].message
    );
    assert_eq!(
        out[0].data.as_ref().unwrap()["hint"].as_str(),
        Some(out[0].message.as_str()),
        "data.hint must carry the same string as the message, disclosure included"
    );
}

#[test]
fn a_marker_shaped_comment_outside_the_lookback_window_is_not_disclosed() {
    // The disclosure window is exactly the suppression window — a comment too far above could never
    // have suppressed, so it must not be blamed for failing to.
    let files = files(&[(
        "api/h.ts",
        "// idempotent-okay: typo\n\n\n\n\nexport function touch(c: any) { return prisma.ping.create({ data: {} }); }\n",
    )]);
    let symbols = with_write_sites(&files, vec![sym("api/h.ts", "touch", 6)]);
    let out = scan_unsafe_read_endpoint(&ScanUnsafeReadEndpointInput {
        api_endpoints: &[endpoint("GET", "/touch", "touch")],
        symbols: &symbols,
        symbol_graph: &Vec::new(),
        files: &files,
    });
    assert_eq!(out.len(), 1, "{out:?}");
    assert!(
        !out[0].message.contains("does not suppress this rule"),
        "{}",
        out[0].message
    );
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
         write, or mark it with `// idempotent-ok: <reason>` on the body-start line or up to 3 lines above if a retry is \
         genuinely safe here. Disable via config `rules: { \"non-idempotent-write\": \"off\" }` \
         (embedders: `disabledRules`) if this applies more broadly. LANGUAGE SIGHTLINE: this check \
         needs store-write evidence that only the TypeScript parser produces \
         (ts/tsx/js/jsx/mjs/cjs/mts/cts) — `SourceSymbol::write_sites`, which \
         parser-python-3/go/rust/csharp/java-21 all leave empty, so a handler in those languages has \
         no write site the call-graph BFS could reach and this rule cannot fire there at all. Easiest \
         to misread: the sibling `mutating-route-no-auth` rule DOES walk Java, so a Java repo can show \
         that rule's findings while this rule stayed dark on the very same routes. ZERO findings of \
         this rule outside ts/tsx/js/jsx/mjs/cjs/mts/cts therefore means NOT ANALYZED, never \"no \
         risky write on these routes\"."
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

// ---------------------------------------------------------------------------------------------
// Policy pin (T2): the marker window is a Rust constant HERE and hand-written English prose in three
// published pages. Neither side can reference the other, so the relationship is sealed instead.
// ---------------------------------------------------------------------------------------------

/// Every published surface that spells the `idempotent-ok` window out in prose. Relative to this crate's
/// manifest dir; a path that stops existing fails this test loudly rather than silently pinning nothing.
const MARKER_WINDOW_PROSE_PAGES: [&str; 3] = [
    "../../../docs/getting-started.md",
    "../../../site/rules.html",
    "../../../site/usage.html",
];

/// Policy pin (T2 — the boundary admits no shared symbol): `OK_MARKER_LOOKBACK_ABOVE` and the window
/// sentence in `docs/getting-started.md` / `site/rules.html` / `site/usage.html` are ONE policy — "how far
/// above the handler may an author put the marker" — spelled twice only because a Markdown or HTML page
/// cannot reference a Rust constant.
///
/// Why it needs a pin at all: this window was ALREADY wrong once. The near-miss disclosure promised "the 4
/// lines above this handler" while `scan_marker_window` reads the body-start line plus 3 above, so a marker
/// placed exactly where the message invited it landed outside the window and did nothing — the silent
/// failure the disclosure exists to prevent, reproduced by the disclosure itself. Nothing was red: the
/// scanner's own tests assert suppression behavior, never the sentence describing it, and the docs are
/// prose no test read.
///
/// Both sides are read from what actually ships — the message from a real finding, the prose from the
/// pages' bytes — so this file holds no third copy to drift. A PHRASE is pinned rather than the bare
/// number because `3` occurs in unrelated prose all over these pages, and a bare-number pin would pass on
/// a sentence about something else entirely.
#[test]
fn the_marker_window_phrase_is_identical_in_the_finding_and_the_published_docs() {
    let phrase = marker_window_phrase();

    let out = unsafe_read_with_comment("// idempotent-okay:");
    let message = &out[0].message;
    assert!(
        message.contains(&phrase),
        "the near-miss disclosure no longer renders the shared window phrase `{phrase}` — it reads: \
         {message}"
    );

    for rel in MARKER_WINDOW_PROSE_PAGES {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("cannot read published page {}: {e}", path.display()));
        assert!(
            text.contains(&phrase),
            "{rel} no longer says `{phrase}` — the marker window and the page that documents it have \
             forked, so an author following the page would place the marker outside the window \
             `scan_marker_window` actually reads (see this test's own doc: that exact defect already \
             shipped once)"
        );
    }
}

/// The three published pages that must carry this module's LANGUAGE SIGHTLINE, in the same shape as
/// [`MARKER_WINDOW_PROSE_PAGES`] above. `docs/rules/catalog.md` is additionally embedded in the shipped
/// binary (`crates/summary/src/contracts.rs`'s `rule-catalog` resource), so its copy is what an MCP client
/// reads without a source checkout.
const SIGHTLINE_PROSE_PAGES: [&str; 3] = [
    "../../../docs/rules/catalog.md",
    "../../../site/rules.html",
    "../../../docs/getting-started.md",
];

/// Policy pin (T2 — a Markdown/HTML page cannot reference a Rust constant): `WRITE_SITE_COVERED_EXTENSIONS`
/// and the sightline sentence on the three published pages are ONE policy — "which languages can these two
/// rules see a write in at all" — spelled four times only because three of the surfaces are prose.
///
/// Why it needs a pin: both rules are structurally silent outside those extensions (`write_sites` is
/// TypeScript-only), and a message only ships ON a finding — so in exactly the repos the sightline is
/// about, the message never renders and the PAGES are the only surface a user can reach. A page that
/// forgets the sightline, or keeps a stale extension list after the write-site producer grows a language,
/// sells the false assurance this whole disclosure exists to stop. Nothing else is red when that happens:
/// the scanners' own tests assert firing behavior, never the sentence describing where they can fire.
///
/// Both sides are read from what actually ships — the claim from a REAL finding's message, the prose from
/// the pages' own bytes — so this file holds no third copy to drift. The pinned fragment is markup-free
/// (see `write_site_sightline_claim`'s doc) and carries the whole extension list, so an unrelated sentence
/// cannot satisfy it by accident the way a bare-token pin could.
#[test]
fn the_write_site_sightline_is_identical_in_the_finding_and_the_published_docs() {
    let claim = write_site_sightline_claim();

    // A real finding from each of the two rules — not a call to the formatter, which would only prove
    // the formatter agrees with itself.
    let unsafe_read = unsafe_read_with_comment("// unrelated");
    assert_eq!(unsafe_read.len(), 1);
    let files = files(&[(
        "api/h.ts",
        "export function putThing(c: any) { return prisma.thing.create({ data: { id: c.id } }); }\n",
    )]);
    let symbols = with_write_sites(&files, vec![sym("api/h.ts", "putThing", 1)]);
    let non_idempotent = scan_non_idempotent_write(&ScanNonIdempotentWriteInput {
        api_endpoints: &[endpoint("PUT", "/things/:id", "putThing")],
        symbols: &symbols,
        symbol_graph: &Vec::new(),
        files: &files,
    });
    assert_eq!(non_idempotent.len(), 1);
    for (rule, out) in [
        ("unsafe-read-endpoint", &unsafe_read),
        ("non-idempotent-write", &non_idempotent),
    ] {
        let message = &out[0].message;
        assert!(
            message.contains(&claim),
            "{rule}'s finding no longer renders the shared sightline claim `{claim}` — it reads: \
             {message}"
        );
        // `data.hint` and `message` are the same string by contract; pin that here too so a future
        // splice cannot disclose on one and stay silent on the other.
        assert_eq!(
            out[0].data.as_ref().unwrap()["hint"].as_str().unwrap(),
            message,
            "{rule}'s data.hint drifted from its message"
        );
    }

    // Whitespace is collapsed before comparing: Markdown and HTML both treat a newline inside a
    // paragraph as a space, so a page that wraps this sentence at its column limit is byte-different
    // but reader-identical. Collapsing compares the WORDS (which is the policy) instead of forcing
    // three prose files to keep one 88-character line unwrapped (which is not).
    let collapse = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    let claim = collapse(&claim);
    for rel in SIGHTLINE_PROSE_PAGES {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
        let text = collapse(
            &std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("cannot read published page {}: {e}", path.display())),
        );
        assert!(
            text.contains(&claim),
            "{rel} no longer says `{claim}` — the page and the rules have forked, so a reader whose \
             repo is Python/Go/Rust/C#/Java is told nothing about why these two rules report zero \
             there, which is the false assurance this sightline exists to prevent"
        );
    }
}

// ── SourceSymbol.id is not unique, and these two builders are where that used to become silence ──

/// `build_name_index` must count DISTINCT ids. Two entries of one id are one candidate, not two, and
/// treating them as two made `resolve_handler_scoped` refuse to guess — so a route whose handler was
/// merely overloaded lost its handler and every rule that judges the handler went quiet.
///
/// Measured 2026-08-11 end to end: a Spring controller with a single `@PostMapping create(String)` is
/// flagged by `mutating-route-no-auth`; adding an ordinary UNANNOTATED overload `create(int, String)`
/// in the same file silenced it, with nothing in `warnings`/`blindSpots`/disclosure classes saying so.
#[test]
fn one_id_declared_twice_is_one_candidate_not_an_ambiguity() {
    let overloaded = vec![sym("C.java", "C.create", 14), sym("C.java", "C.create", 19)];
    let idx = build_name_index(&overloaded);
    assert_eq!(
        idx["create"].len(),
        1,
        "same id twice is one candidate: {:?}",
        idx["create"]
    );
    assert_eq!(
        resolve_handler("create", &idx).as_deref(),
        Some("C.java#C.create"),
        "an overload must not cost the route its handler"
    );

    // The control: two DIFFERENT ids under one tail name stay ambiguous, and the do-not-guess rule
    // still returns None. Without this the dedup could have been a blanket "always pick one".
    let two_files = vec![sym("A.java", "A.create", 3), sym("B.java", "B.create", 3)];
    let idx = build_name_index(&two_files);
    assert_eq!(idx["create"].len(), 2);
    assert_eq!(
        resolve_handler("create", &idx),
        None,
        "genuine ambiguity is still refused"
    );
}

/// `symbols_by_id` must not let a later same-id declaration with no write sites displace the one that
/// has them. `HashMap::collect` keeps the LAST entry, which is what both call sites used to do.
///
/// The shape is ordinary TypeScript: an `interface helper` declaration-merged onto `function helper()`.
/// Measured 2026-08-11: `unsafe-read-endpoint` fired when the interface came first and vanished when it
/// came second — declaration order is not something this rule may depend on.
#[test]
fn a_same_id_declaration_without_write_sites_never_displaces_the_one_that_has_them() {
    let files = files(&[(
        "h.ts",
        "export function helper() { db.create({}); }
",
    )]);
    let real = with_write_sites(&files, vec![sym("h.ts", "helper", 1)]);
    assert!(
        !real[0].write_sites.is_empty(),
        "fixture must actually carry a write site"
    );
    let empty = sym("h.ts", "helper", 1);
    assert_eq!(
        real[0].id, empty.id,
        "the whole point is that the ids collide"
    );

    for (order, symbols) in [
        (
            "write-site symbol LAST",
            vec![empty.clone(), real[0].clone()],
        ),
        (
            "write-site symbol FIRST",
            vec![real[0].clone(), empty.clone()],
        ),
    ] {
        let by_id = symbols_by_id(&symbols);
        assert!(
            !by_id["h.ts#helper"].write_sites.is_empty(),
            "{order}: the entry carrying write sites must win regardless of declaration order"
        );
    }
}

/// `scan_marker_window` must read EVERY same-id declaration's window, not just the first.
///
/// The fourth consumer of the non-unique `SourceSymbol.id` (`file#name`): `2c34e6e` fixed
/// `build_name_index`, `symbols_by_id` and the two inline id maps, and left this one a bare `.find()`.
///
/// Measured before the fix, on two `touch` declarations sharing one id where the SECOND carries the
/// write: no marker -> 1 finding, and a correctly placed `// idempotent-ok:` on that second
/// declaration -> 1 finding as well. The marker the rule's own message tells the author to write was
/// read against the FIRST declaration's window and did nothing.
#[test]
fn a_marker_under_a_same_id_sibling_declaration_still_suppresses() {
    let write = "export function touch(c: any) { return prisma.ping.create({ data: {} }); }";
    // Line 4 (the write-carrying declaration) is the one the marker is placed above.
    let control =
        format!("export function touch(c: any) {{ return 1; }}\n\n// plain comment\n{write}\n");
    let marked = format!(
        "export function touch(c: any) {{ return 1; }}\n\n// idempotent-ok: audit trail\n{write}\n"
    );

    let run = |text: &str| {
        let files = files(&[("api/h.ts", text)]);
        let symbols = with_write_sites(
            &files,
            vec![sym("api/h.ts", "touch", 1), sym("api/h.ts", "touch", 4)],
        );
        scan_unsafe_read_endpoint(&ScanUnsafeReadEndpointInput {
            api_endpoints: &[endpoint("GET", "/touch", "touch")],
            symbols: &symbols,
            symbol_graph: &Vec::new(),
            files: &files,
        })
        .len()
    };

    // The control proves the fixture is live — without it, "0 findings" would pass for the wrong reason.
    assert_eq!(
        run(&control),
        1,
        "control: the write is found and the rule fires"
    );
    assert_eq!(
        run(&marked),
        0,
        "a marker in a same-id declaration's own window must suppress"
    );
}

/// The near-miss disclosure reads through the same window scan, so it was blind in the same shape —
/// the author got neither the suppression they wrote nor the sentence saying it did not work.
#[test]
fn a_near_miss_marker_under_a_same_id_sibling_declaration_is_still_disclosed() {
    let text = "export function touch(c: any) { return 1; }\n\n// idempotent-okay: typo'd stem\nexport function touch(c: any) { return prisma.ping.create({ data: {} }); }\n";
    let files = files(&[("api/h.ts", text)]);
    let symbols = with_write_sites(
        &files,
        vec![sym("api/h.ts", "touch", 1), sym("api/h.ts", "touch", 4)],
    );
    let out = scan_unsafe_read_endpoint(&ScanUnsafeReadEndpointInput {
        api_endpoints: &[endpoint("GET", "/touch", "touch")],
        symbols: &symbols,
        symbol_graph: &Vec::new(),
        files: &files,
    });
    // Still fires (a near-miss never suppresses) — but now it SAYS the comment does not suppress.
    assert_eq!(out.len(), 1);
    assert!(
        out[0].message.contains("idempotent-okay")
            && out[0].message.contains("does not suppress this rule"),
        "the near-miss sentence must name the token it found: {}",
        out[0].message
    );
}
