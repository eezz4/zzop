//! `scan_unsafe_read_endpoint` + `scan_non_idempotent_write` — the two native whole-graph rules in this
//! crate that need call-graph BFS. `apiChurn` (needs git-history joins) and `feBeSpecDrift` (cross-service
//! type drift) are out of scope: both need capabilities beyond a single-repo call graph.
//!
//! Both scanners resolve a method-gated `ApiEndpoint`'s handler to a symbol id, then BFS downstream over
//! the whole-repo `SymbolGraph` (`zpz_core::callgraph::bfs_reachable`) until a symbol whose body contains a
//! flaggable write pattern is found (lowest depth wins; ties break by symbol id ascending). This crate has
//! no parser dependency, so write-site detection is a regex scan over the reached symbol's raw text span
//! rather than an AST walk: it matches a bare identifier receiver, or exactly one `base.member` level
//! before the write call (a 3+-level chain is unmatched). Two narrowing consequences: a nested function's
//! body is included in its outer symbol's scanned span, so a write inside it attributes to the outer
//! symbol; and a raw-SQL label truncates at the first newline, so a multi-line statement's label can be
//! incomplete.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::OnceLock;

use regex::{Match, Regex};
use zpz_core::callgraph::{bfs_reachable, SymbolGraph};
use zpz_core::{ApiEndpoint, Finding, Severity, SourceSymbol};

/// Default ORM-receiver pattern: a Repository/Store suffix, or a bare prisma/db/orm/tx/trx identifier.
pub const DEFAULT_ORM_RECEIVER_PATTERN: &str = r"Repository$|Store$|^prisma$|^db$|^orm$|^tx$|^trx$";

/// Default write-method vocabulary for `scan_unsafe_read_endpoint`, overridden via `write_methods`.
pub const DEFAULT_WRITE_METHODS: &[&str] = &[
    "create",
    "createMany",
    "update",
    "updateMany",
    "delete",
    "deleteMany",
    "upsert",
    "insert",
    "save",
    "remove",
];

/// Lines above a handler's body start to look back for an `idempotent-ok` marker (shared by both scanners).
const OK_MARKER_LOOKBACK_LINES: u32 = 4;
const WRITE_LABEL_TOKEN_COUNT: usize = 3;

const SAFE_METHODS: [&str; 2] = ["GET", "HEAD"];
const WRITE_HTTP_METHODS: [&str; 4] = ["PUT", "DELETE", "POST", "PATCH"];

const CREATE_METHODS: [&str; 3] = ["create", "createMany", "insert"];
const UPDATE_METHODS: [&str; 3] = ["update", "updateMany", "upsert"];
const COUNTER_METHODS: [&str; 4] = ["incr", "incrby", "decr", "decrby"];
const ATOMIC_OP_KEYS: [&str; 4] = ["increment", "decrement", "push", "multiply"];

fn ok_marker_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"//\s*idempotent-ok:").unwrap())
}

/// Raw-SQL write in a string literal — covers stacks issuing SQL directly (Cloudflare D1, better-sqlite3,
/// `pg`) rather than through a store-method call. Never matches a SELECT.
fn sql_write_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(
            r"(?i)\b(?:INSERT\s+(?:OR\s+\w+\s+)?INTO|UPDATE\s+\w|DELETE\s+FROM|REPLACE\s+INTO)\b",
        )
        .unwrap()
    })
}

fn atomic_op_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(&format!(r"\b(?:{})\s*:", ATOMIC_OP_KEYS.join("|"))).unwrap())
}

/// A store-shaped method call, receiver/method captured generically (classified in `symbol_bad_sites`).
fn method_call_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(r"\b([A-Za-z_$][\w$]*)(?:\.[A-Za-z_$][\w$]*)?\.([A-Za-z_$][\w$]*)\s*\(").unwrap()
    })
}

/// Write-method-vocab-driven variant of `method_call_re`, for `scan_unsafe_read_endpoint`'s configurable
/// write-methods list.
fn write_call_re(methods: &[String]) -> Regex {
    let alts: Vec<String> = methods.iter().map(|m| regex::escape(m)).collect();
    Regex::new(&format!(
        r"\b([A-Za-z_$][\w$]*)(?:\.[A-Za-z_$][\w$]*)?\.(?:{})\s*\(",
        alts.join("|")
    ))
    .unwrap()
}

// --- Shared helpers (name index / handler resolution / whitelist / span text) ---

fn ident_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"[A-Za-z_$][\w$]*").unwrap())
}

/// Tail name (after the last `.`) -> symbol ids (`"file#name"`). `pub(crate)`: also used by `mutating_route_no_auth`.
pub(crate) fn build_name_index(symbols: &[SourceSymbol]) -> HashMap<String, Vec<String>> {
    let mut idx: HashMap<String, Vec<String>> = HashMap::new();
    for s in symbols {
        let tail = s.name.rsplit('.').next().unwrap_or(&s.name).to_string();
        idx.entry(tail).or_default().push(s.id.clone());
    }
    idx
}

/// Resolves a handler reference string to a unique symbol id, stripping wrapper calls (`rateLimit(fn)`) and
/// member access (`ctrl.list`). `None` when unknown or ambiguous (defined in multiple files) — never guessed.
pub(crate) fn resolve_handler(handler: &str, idx: &HashMap<String, Vec<String>>) -> Option<String> {
    let ids: Vec<&str> = ident_re().find_iter(handler).map(|m| m.as_str()).collect();
    for ident in ids.iter().rev() {
        match idx.get(*ident) {
            Some(candidates) if candidates.len() == 1 => return Some(candidates[0].clone()),
            Some(_) => return None, // ambiguous — do not guess
            None => continue,
        }
    }
    None
}

/// A `// idempotent-ok: <reason>` comment within `OK_MARKER_LOOKBACK_LINES` lines above the handler's body suppresses the finding (also covers the declaration's own line, an off-by-one side effect).
fn is_whitelisted(
    handler_symbol: &str,
    symbols: &[SourceSymbol],
    files: &HashMap<String, String>,
) -> bool {
    let Some(sym) = symbols.iter().find(|s| s.id == handler_symbol) else {
        return false;
    };
    let Some(text) = files.get(&sym.file) else {
        return false;
    };
    let lines: Vec<&str> = text.split('\n').collect();
    let decl_line = sym.body_start.unwrap_or(sym.line);
    let start = decl_line.saturating_sub(OK_MARKER_LOOKBACK_LINES);
    let mut i = start;
    while i < decl_line {
        if let Some(l) = lines.get(i as usize) {
            if ok_marker_re().is_match(l) {
                return true;
            }
        }
        i += 1;
    }
    false
}

/// Joins `text`'s lines `body_start..=body_end` (1-based) into one block, plus `body_start` for `line_at`.
fn symbol_span_text(text: &str, body_start: u32, body_end: u32) -> (String, u32) {
    let lines: Vec<&str> = text.split('\n').collect();
    let start_idx = (body_start.saturating_sub(1)) as usize;
    let end_idx = (body_end as usize).min(lines.len());
    if start_idx >= end_idx {
        return (String::new(), body_start);
    }
    (lines[start_idx..end_idx].join("\n"), body_start)
}

/// Absolute 1-based line for a byte `offset` into a `symbol_span_text` block.
fn line_at(block: &str, first_line: u32, offset: usize) -> u32 {
    let capped = offset.min(block.len());
    first_line + block[..capped].matches('\n').count() as u32
}

/// The bracket-balanced argument text after a call's `(` (`open_after` = byte offset right after it, at depth 1).
fn call_args_span(block: &str, open_after: usize) -> &str {
    let mut depth = 1i32;
    let mut pos = open_after;
    let mut end = block.len();
    for c in block[open_after..].chars() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    end = pos;
                    break;
                }
            }
            _ => {}
        }
        pos += c.len_utf8();
    }
    &block[open_after..end]
}

fn args_have_atomic_op(args: &str) -> bool {
    atomic_op_re().is_match(args)
}

fn write_sink_label(matched: &str) -> String {
    matched
        .trim_end()
        .trim_end_matches('(')
        .trim_end()
        .to_string()
}

/// A short "verb + next token(s)" label for a raw-SQL write match, truncated at the first newline.
fn sql_label(rest_from_match_start: &str) -> String {
    rest_from_match_start
        .chars()
        .take_while(|&c| c != '\n')
        .collect::<String>()
        .replace(['"', '\'', '`'], "")
        .split_whitespace()
        .take(WRITE_LABEL_TOKEN_COUNT)
        .collect::<Vec<_>>()
        .join(" ")
}

// --- scan_unsafe_read_endpoint ---

/// Input for [`scan_unsafe_read_endpoint`]. `write_methods`/`orm_receiver_pattern` are explicit params —
/// callers pass [`DEFAULT_WRITE_METHODS`] / [`DEFAULT_ORM_RECEIVER_PATTERN`] for the default vocabulary.
pub struct ScanUnsafeReadEndpointInput<'a> {
    pub api_endpoints: &'a [ApiEndpoint],
    pub symbols: &'a [SourceSymbol],
    pub symbol_graph: &'a SymbolGraph,
    /// rel path -> full source text, for both write-site scanning and the `idempotent-ok` whitelist lookback.
    pub files: &'a HashMap<String, String>,
    pub write_methods: &'a [String],
    pub orm_receiver_pattern: &'a str,
}

#[derive(Debug, Clone)]
struct WriteSite {
    file: String,
    line: u32,
    sink: String,
}

/// Flags a "safe" method endpoint (GET/HEAD) whose handler reaches a database write — per RFC 7231, GET/HEAD
/// must be safe and idempotent, so a mutating read is a crawler/prefetch/retry hazard.
pub fn scan_unsafe_read_endpoint(input: &ScanUnsafeReadEndpointInput) -> Vec<Finding> {
    let reads: Vec<&ApiEndpoint> = input
        .api_endpoints
        .iter()
        .filter(|e| SAFE_METHODS.contains(&e.method.to_uppercase().as_str()))
        .collect();
    if reads.is_empty() {
        return Vec::new();
    }

    let name_index = build_name_index(input.symbols);
    let symbols_by_id: HashMap<&str, &SourceSymbol> =
        input.symbols.iter().map(|s| (s.id.as_str(), s)).collect();
    let write_re = write_call_re(input.write_methods);
    let orm_re = Regex::new(input.orm_receiver_pattern)
        .unwrap_or_else(|_| Regex::new(DEFAULT_ORM_RECEIVER_PATTERN).unwrap());

    let cache: RefCell<HashMap<String, Option<WriteSite>>> = RefCell::new(HashMap::new());
    let site_at = |id: &str| -> Option<WriteSite> {
        if let Some(hit) = cache.borrow().get(id) {
            return hit.clone();
        }
        let site = symbols_by_id.get(id).and_then(|s| {
            input
                .files
                .get(&s.file)
                .and_then(|text| symbol_write_site(s, text, &write_re, &orm_re))
        });
        cache.borrow_mut().insert(id.to_string(), site.clone());
        site
    };

    let mut out = Vec::new();
    for e in reads {
        let Some(handler_symbol) = resolve_handler(&e.handler, &name_index) else {
            continue; // unresolved handler — do not guess
        };
        if is_whitelisted(&handler_symbol, input.symbols, input.files) {
            continue;
        }
        let Some((write_id, depth)) = bfs_reachable(input.symbol_graph, &handler_symbol, |id| {
            site_at(id).is_some()
        }) else {
            continue;
        };
        let site = site_at(&write_id).expect("predicate true implies a site exists");
        let method = e.method.to_uppercase();
        let where_ = if depth == 0 {
            format!("{method} {} writes directly ({})", e.path, site.sink)
        } else {
            format!(
                "{method} {} reaches a write ({}) {depth} call(s) deep",
                e.path, site.sink
            )
        };
        let hint = format!(
            "{where_} — GET/HEAD must be safe & idempotent. Move the write behind a mutating method \
             (POST/PUT/PATCH/DELETE), or make this endpoint genuinely read-only. If the write is \
             deliberate and safe to repeat (e.g. a fire-and-forget audit log), mark it with \
             `// idempotent-ok: <reason>` on the line above the handler, or disable via rule config \
             `disabled_rules: [\"unsafe-read-endpoint\"]` if this applies more broadly."
        );
        out.push(Finding {
            rule_id: "unsafe-read-endpoint".to_string(),
            severity: Severity::Warning,
            file: site.file.clone(),
            line: site.line,
            message: hint.clone(),
            data: Some(serde_json::json!({
                "method": method,
                "path": e.path,
                "handler": e.handler,
                "handlerSymbol": handler_symbol,
                "writeSymbol": write_id,
                "writeFile": site.file,
                "writeLine": site.line,
                "sink": site.sink,
                "depth": depth,
                "hint": hint,
            })),
        });
    }
    out
}

/// Locates the first (in source order) qualifying write in a symbol's body — a raw-SQL write string or a
/// store-like write call, whichever comes first.
fn symbol_write_site(
    sym: &SourceSymbol,
    text: &str,
    write_re: &Regex,
    orm_re: &Regex,
) -> Option<WriteSite> {
    let (start, end) = (sym.body_start?, sym.body_end?);
    let (block, first_line) = symbol_span_text(text, start, end);
    if block.is_empty() {
        return None;
    }

    let sql_m = sql_write_re().find(&block);
    let write_m = first_orm_write_match(&block, write_re, orm_re);

    let use_sql = match (&sql_m, &write_m) {
        (Some(s), Some(w)) => s.start() <= w.start(),
        (Some(_), None) => true,
        (None, _) => false,
    };

    if use_sql {
        let m = sql_m?;
        return Some(WriteSite {
            file: sym.file.clone(),
            line: line_at(&block, first_line, m.start()),
            sink: sql_label(&block[m.start()..]),
        });
    }
    let m = write_m?;
    Some(WriteSite {
        file: sym.file.clone(),
        line: line_at(&block, first_line, m.start()),
        sink: write_sink_label(m.as_str()),
    })
}

/// The leftmost match of `write_re` in `block` whose captured base receiver satisfies `orm_re` — a call
/// whose method name matches the vocab but whose receiver isn't store-like is skipped, not treated as a match.
fn first_orm_write_match<'t>(
    block: &'t str,
    write_re: &Regex,
    orm_re: &Regex,
) -> Option<Match<'t>> {
    write_re
        .captures_iter(block)
        .find(|caps| orm_re.is_match(&caps[1]))
        .map(|caps| caps.get(0).unwrap())
}

// --- scan_non_idempotent_write ---

/// Input for [`scan_non_idempotent_write`]. Unlike `scan_unsafe_read_endpoint`, this scanner's
/// create/update/counter method sets are fixed — only the ORM-receiver pattern is caller-supplied.
pub struct ScanNonIdempotentWriteInput<'a> {
    pub api_endpoints: &'a [ApiEndpoint],
    pub symbols: &'a [SourceSymbol],
    pub symbol_graph: &'a SymbolGraph,
    pub files: &'a HashMap<String, String>,
    pub orm_receiver_pattern: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NonIdempotentKind {
    Create,
    AtomicAccumulate,
    Counter,
}

impl NonIdempotentKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::AtomicAccumulate => "atomic-accumulate",
            Self::Counter => "counter",
        }
    }
}

#[derive(Debug, Clone)]
struct BadSite {
    file: String,
    line: u32,
    sink: String,
    kind: NonIdempotentKind,
}

/// Which finding kinds apply to a method: `create` only matters for PUT/DELETE (idempotency-promising); POST/PATCH are flagged only for accumulation.
fn flaggable_kinds(method: &str) -> &'static [NonIdempotentKind] {
    if method == "PUT" || method == "DELETE" {
        &[
            NonIdempotentKind::Create,
            NonIdempotentKind::AtomicAccumulate,
            NonIdempotentKind::Counter,
        ]
    } else {
        &[
            NonIdempotentKind::AtomicAccumulate,
            NonIdempotentKind::Counter,
        ]
    }
}

/// Flags a write handler that reaches a non-idempotent operation: `create`/`insert` on PUT/DELETE (a retry
/// duplicates), or an atomic accumulation/counter bump on any write method (a retry doubles the effect).
pub fn scan_non_idempotent_write(input: &ScanNonIdempotentWriteInput) -> Vec<Finding> {
    let writes: Vec<&ApiEndpoint> = input
        .api_endpoints
        .iter()
        .filter(|e| WRITE_HTTP_METHODS.contains(&e.method.to_uppercase().as_str()))
        .collect();
    if writes.is_empty() {
        return Vec::new();
    }

    let name_index = build_name_index(input.symbols);
    let symbols_by_id: HashMap<&str, &SourceSymbol> =
        input.symbols.iter().map(|s| (s.id.as_str(), s)).collect();
    let orm_re = Regex::new(input.orm_receiver_pattern)
        .unwrap_or_else(|_| Regex::new(DEFAULT_ORM_RECEIVER_PATTERN).unwrap());

    let cache: RefCell<HashMap<String, Vec<BadSite>>> = RefCell::new(HashMap::new());
    let sites_at = |id: &str| -> Vec<BadSite> {
        if let Some(hit) = cache.borrow().get(id) {
            return hit.clone();
        }
        let sites = symbols_by_id
            .get(id)
            .and_then(|s| {
                input
                    .files
                    .get(&s.file)
                    .map(|text| symbol_bad_sites(s, text, &orm_re))
            })
            .unwrap_or_default();
        cache.borrow_mut().insert(id.to_string(), sites.clone());
        sites
    };

    let mut out = Vec::new();
    for e in writes {
        let method = e.method.to_uppercase();
        let allowed = flaggable_kinds(&method);
        let Some(handler_symbol) = resolve_handler(&e.handler, &name_index) else {
            continue;
        };
        if is_whitelisted(&handler_symbol, input.symbols, input.files) {
            continue;
        }
        let Some((id, depth)) = bfs_reachable(input.symbol_graph, &handler_symbol, |id| {
            sites_at(id).iter().any(|s| allowed.contains(&s.kind))
        }) else {
            continue;
        };
        let site = sites_at(&id)
            .into_iter()
            .find(|s| allowed.contains(&s.kind))
            .expect("predicate true implies a matching site exists");

        let hint = hint_for(&method, &e.path, &site, depth);
        out.push(Finding {
            rule_id: "non-idempotent-write".to_string(),
            severity: Severity::Warning,
            file: site.file.clone(),
            line: site.line,
            message: hint.clone(),
            data: Some(serde_json::json!({
                "method": method,
                "path": e.path,
                "handler": e.handler,
                "handlerSymbol": handler_symbol,
                "writeSymbol": id,
                "writeFile": site.file,
                "writeLine": site.line,
                "sink": site.sink,
                "kind": site.kind.as_str(),
                "depth": depth,
                "hint": hint,
            })),
        });
    }
    out
}

fn hint_for(method: &str, path: &str, site: &BadSite, depth: u32) -> String {
    let where_ = if depth == 0 {
        "directly".to_string()
    } else {
        format!("{depth} call(s) deep")
    };
    let why = match site.kind {
        NonIdempotentKind::Create => "a retry inserts a duplicate row",
        NonIdempotentKind::AtomicAccumulate => {
            "a retry applies the increment again (doubles the effect)"
        }
        NonIdempotentKind::Counter => "a retry bumps the counter again",
    };
    let contract = if method == "PUT" || method == "DELETE" {
        format!("{method} must be idempotent")
    } else {
        format!("a retried {method} must converge or carry an idempotency key")
    };
    format!(
        "{method} {path} reaches {} {where_} ({}) — {why}; {contract}. Add an idempotency key or a \
         dedup/uniqueness check before the write, or mark it with `// idempotent-ok: <reason>` above the \
         handler if a retry is genuinely safe here. Disable via rule config \
         `disabled_rules: [\"non-idempotent-write\"]` if this applies more broadly.",
        site.sink,
        site.kind.as_str()
    )
}

fn symbol_bad_sites(sym: &SourceSymbol, text: &str, orm_re: &Regex) -> Vec<BadSite> {
    let Some((start, end)) = sym.body_start.zip(sym.body_end) else {
        return Vec::new();
    };
    let (block, first_line) = symbol_span_text(text, start, end);
    if block.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::new();
    for caps in method_call_re().captures_iter(&block) {
        let base = &caps[1];
        if !orm_re.is_match(base) {
            continue;
        }
        let method = &caps[2];
        let lower = method.to_lowercase();
        let m0 = caps.get(0).unwrap();
        let kind = if CREATE_METHODS.contains(&method) {
            Some(NonIdempotentKind::Create)
        } else if COUNTER_METHODS.contains(&lower.as_str()) {
            Some(NonIdempotentKind::Counter)
        } else if UPDATE_METHODS.contains(&method) {
            let args = call_args_span(&block, m0.end());
            args_have_atomic_op(args).then_some(NonIdempotentKind::AtomicAccumulate)
        } else {
            None
        };
        let Some(kind) = kind else { continue };
        out.push(BadSite {
            file: sym.file.clone(),
            line: line_at(&block, first_line, m0.start()),
            sink: write_sink_label(m0.as_str()),
            kind,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    //! Tests for `scan_unsafe_read_endpoint` and `scan_non_idempotent_write`, using hand-built fixtures (no
    //! parser). Every fixture body is single-line, so `body_start == body_end == <declaration line>`.
    use super::*;
    use zpz_core::callgraph::SymbolEdge;
    use zpz_core::SourceSymbolKind;

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
        }
    }

    fn endpoint(method: &str, path: &str, handler: &str) -> ApiEndpoint {
        ApiEndpoint {
            method: method.to_string(),
            path: path.to_string(),
            handler: handler.to_string(),
            drift_ok: false,
        }
    }

    fn edge(from: &str, to: &str) -> SymbolEdge {
        SymbolEdge {
            from: from.to_string(),
            to: to.to_string(),
        }
    }

    fn write_methods() -> Vec<String> {
        DEFAULT_WRITE_METHODS
            .iter()
            .map(|s| s.to_string())
            .collect()
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
        let symbols = vec![
            sym("api/handlers.ts", "activateUser", 1),
            sym("api/handlers.ts", "getUser", 2),
            sym("api/service.ts", "activate", 1),
        ];
        let graph = vec![edge(
            "api/handlers.ts#activateUser",
            "api/service.ts#activate",
        )];
        let endpoints = vec![
            endpoint("GET", "/users/:id/activate", "activateUser"),
            endpoint("GET", "/users/:id", "getUser"),
        ];
        let wm = write_methods();
        let out = scan_unsafe_read_endpoint(&ScanUnsafeReadEndpointInput {
            api_endpoints: &endpoints,
            symbols: &symbols,
            symbol_graph: &graph,
            files: &files,
            write_methods: &wm,
            orm_receiver_pattern: DEFAULT_ORM_RECEIVER_PATTERN,
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
        let symbols = vec![sym("api/h.ts", "touch", 1)];
        let out = scan_unsafe_read_endpoint(&ScanUnsafeReadEndpointInput {
            api_endpoints: &[endpoint("GET", "/touch", "touch")],
            symbols: &symbols,
            symbol_graph: &Vec::new(),
            files: &files,
            write_methods: &write_methods(),
            orm_receiver_pattern: DEFAULT_ORM_RECEIVER_PATTERN,
        });
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].data.as_ref().unwrap()["depth"], 0);
    }

    #[test]
    fn non_safe_methods_are_never_flagged_even_when_they_write() {
        let files = files(&[(
            "api/h.ts",
            "export function create(c: any) { return prisma.user.create({ data: {} }); }\n",
        )]);
        let symbols = vec![sym("api/h.ts", "create", 1)];
        let out = scan_unsafe_read_endpoint(&ScanUnsafeReadEndpointInput {
            api_endpoints: &[endpoint("POST", "/users", "create")],
            symbols: &symbols,
            symbol_graph: &Vec::new(),
            files: &files,
            write_methods: &write_methods(),
            orm_receiver_pattern: DEFAULT_ORM_RECEIVER_PATTERN,
        });
        assert!(out.is_empty());
    }

    #[test]
    fn read_only_get_handler_has_no_finding() {
        let files = files(&[(
            "api/h.ts",
            "export function list(c: any) { return prisma.user.findMany(); }\n",
        )]);
        let symbols = vec![sym("api/h.ts", "list", 1)];
        let out = scan_unsafe_read_endpoint(&ScanUnsafeReadEndpointInput {
            api_endpoints: &[endpoint("GET", "/users", "list")],
            symbols: &symbols,
            symbol_graph: &Vec::new(),
            files: &files,
            write_methods: &write_methods(),
            orm_receiver_pattern: DEFAULT_ORM_RECEIVER_PATTERN,
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
        let symbols = vec![
            sym("api/h.ts", "getRates", 1),
            sym("api/refresh.ts", "refresh", 1),
        ];
        let graph = vec![edge("api/h.ts#getRates", "api/refresh.ts#refresh")];
        let out = scan_unsafe_read_endpoint(&ScanUnsafeReadEndpointInput {
            api_endpoints: &[endpoint("GET", "/api/rates", "getRates")],
            symbols: &symbols,
            symbol_graph: &graph,
            files: &files,
            write_methods: &write_methods(),
            orm_receiver_pattern: DEFAULT_ORM_RECEIVER_PATTERN,
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
        let symbols = vec![sym("api/h.ts", "list", 1)];
        let out = scan_unsafe_read_endpoint(&ScanUnsafeReadEndpointInput {
            api_endpoints: &[endpoint("GET", "/api/rates", "list")],
            symbols: &symbols,
            symbol_graph: &Vec::new(),
            files: &files,
            write_methods: &write_methods(),
            orm_receiver_pattern: DEFAULT_ORM_RECEIVER_PATTERN,
        });
        assert!(out.is_empty());
    }

    #[test]
    fn idempotent_ok_marker_above_the_handler_suppresses_the_finding() {
        let files = files(&[(
            "api/h.ts",
            "// idempotent-ok: write is a fire-and-forget audit log, safe to repeat\nexport function touch(c: any) { return prisma.ping.create({ data: {} }); }\n",
        )]);
        let symbols = vec![sym("api/h.ts", "touch", 2)];
        let out = scan_unsafe_read_endpoint(&ScanUnsafeReadEndpointInput {
            api_endpoints: &[endpoint("GET", "/touch", "touch")],
            symbols: &symbols,
            symbol_graph: &Vec::new(),
            files: &files,
            write_methods: &write_methods(),
            orm_receiver_pattern: DEFAULT_ORM_RECEIVER_PATTERN,
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
        let symbols = vec![sym("api/a.ts", "dup", 1), sym("api/b.ts", "dup", 1)];
        let out = scan_unsafe_read_endpoint(&ScanUnsafeReadEndpointInput {
            api_endpoints: &[endpoint("GET", "/x", "dup")],
            symbols: &symbols,
            symbol_graph: &Vec::new(),
            files: &files,
            write_methods: &write_methods(),
            orm_receiver_pattern: DEFAULT_ORM_RECEIVER_PATTERN,
        });
        assert!(out.is_empty());
    }

    #[test]
    fn wrapped_handler_resolves_to_the_inner_identifier() {
        let files = files(&[(
            "api/h.ts",
            "export function getThing(c: any) { return prisma.thing.delete({ where: { id: 1 } }); }\n",
        )]);
        let symbols = vec![sym("api/h.ts", "getThing", 1)];
        let out = scan_unsafe_read_endpoint(&ScanUnsafeReadEndpointInput {
            api_endpoints: &[endpoint("GET", "/thing", "rateLimit(getThing)")],
            symbols: &symbols,
            symbol_graph: &Vec::new(),
            files: &files,
            write_methods: &write_methods(),
            orm_receiver_pattern: DEFAULT_ORM_RECEIVER_PATTERN,
        });
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].data.as_ref().unwrap()["sink"], "prisma.thing.delete");
    }

    // --- scan_non_idempotent_write ---

    #[test]
    fn put_handler_that_creates_a_row_is_flagged_kind_create() {
        let files = files(&[("api/h.ts", "export function putThing(c: any) { return prisma.thing.create({ data: { id: c.id } }); }\n")]);
        let symbols = vec![sym("api/h.ts", "putThing", 1)];
        let out = scan_non_idempotent_write(&ScanNonIdempotentWriteInput {
            api_endpoints: &[endpoint("PUT", "/things/:id", "putThing")],
            symbols: &symbols,
            symbol_graph: &Vec::new(),
            files: &files,
            orm_receiver_pattern: DEFAULT_ORM_RECEIVER_PATTERN,
        });
        assert_eq!(out.len(), 1);
        let data = out[0].data.as_ref().unwrap();
        assert_eq!(data["method"], "PUT");
        assert_eq!(data["kind"], "create");
        assert_eq!(data["sink"], "prisma.thing.create");
        assert_eq!(data["depth"], 0);
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
        let symbols = vec![
            sym("api/h.ts", "removeThing", 1),
            sym("api/audit.ts", "log", 1),
        ];
        let graph = vec![edge("api/h.ts#removeThing", "api/audit.ts#log")];
        let out = scan_non_idempotent_write(&ScanNonIdempotentWriteInput {
            api_endpoints: &[endpoint("DELETE", "/things/:id", "removeThing")],
            symbols: &symbols,
            symbol_graph: &graph,
            files: &files,
            orm_receiver_pattern: DEFAULT_ORM_RECEIVER_PATTERN,
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
        let symbols = vec![sym("api/h.ts", "bump", 1)];
        let out = scan_non_idempotent_write(&ScanNonIdempotentWriteInput {
            api_endpoints: &[endpoint("PUT", "/counter/:id", "bump")],
            symbols: &symbols,
            symbol_graph: &Vec::new(),
            files: &files,
            orm_receiver_pattern: DEFAULT_ORM_RECEIVER_PATTERN,
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
        let symbols = vec![sym("api/h.ts", "setName", 1)];
        let out = scan_non_idempotent_write(&ScanNonIdempotentWriteInput {
            api_endpoints: &[endpoint("PUT", "/users/:id", "setName")],
            symbols: &symbols,
            symbol_graph: &Vec::new(),
            files: &files,
            orm_receiver_pattern: DEFAULT_ORM_RECEIVER_PATTERN,
        });
        assert!(out.is_empty());
    }

    #[test]
    fn put_using_upsert_is_not_flagged() {
        let files = files(&[(
            "api/h.ts",
            "export function put(c: any) { return prisma.profile.upsert({ where: { id: c.id }, create: { id: c.id }, update: { name: c.name } }); }\n",
        )]);
        let symbols = vec![sym("api/h.ts", "put", 1)];
        let out = scan_non_idempotent_write(&ScanNonIdempotentWriteInput {
            api_endpoints: &[endpoint("PUT", "/profile/:id", "put")],
            symbols: &symbols,
            symbol_graph: &Vec::new(),
            files: &files,
            orm_receiver_pattern: DEFAULT_ORM_RECEIVER_PATTERN,
        });
        assert!(out.is_empty());
    }

    #[test]
    fn counter_bump_via_a_store_like_receiver_is_flagged_kind_counter() {
        let files = files(&[(
            "api/h.ts",
            "export function put(c: any) { return rateStore.incr(c.key); }\n",
        )]);
        let symbols = vec![sym("api/h.ts", "put", 1)];
        let out = scan_non_idempotent_write(&ScanNonIdempotentWriteInput {
            api_endpoints: &[endpoint("PUT", "/rate/:key", "put")],
            symbols: &symbols,
            symbol_graph: &Vec::new(),
            files: &files,
            orm_receiver_pattern: DEFAULT_ORM_RECEIVER_PATTERN,
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
        let symbols = vec![sym("api/h.ts", "create", 1)];
        let out = scan_non_idempotent_write(&ScanNonIdempotentWriteInput {
            api_endpoints: &[
                endpoint("POST", "/users", "create"),
                endpoint("GET", "/users", "create"),
            ],
            symbols: &symbols,
            symbol_graph: &Vec::new(),
            files: &files,
            orm_receiver_pattern: DEFAULT_ORM_RECEIVER_PATTERN,
        });
        assert!(out.is_empty());
    }

    #[test]
    fn post_with_atomic_increment_is_flagged_regardless_of_method() {
        let files = files(&[(
            "api/h.ts",
            "export function vote(c: any) { return prisma.poll.update({ where: { id: c.id }, data: { votes: { increment: 1 } } }); }\n",
        )]);
        let symbols = vec![sym("api/h.ts", "vote", 1)];
        let out = scan_non_idempotent_write(&ScanNonIdempotentWriteInput {
            api_endpoints: &[endpoint("POST", "/polls/:id/vote", "vote")],
            symbols: &symbols,
            symbol_graph: &Vec::new(),
            files: &files,
            orm_receiver_pattern: DEFAULT_ORM_RECEIVER_PATTERN,
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
        let symbols = vec![sym("api/h.ts", "put", 2)];
        let out = scan_non_idempotent_write(&ScanNonIdempotentWriteInput {
            api_endpoints: &[endpoint("PUT", "/things/:id", "put")],
            symbols: &symbols,
            symbol_graph: &Vec::new(),
            files: &files,
            orm_receiver_pattern: DEFAULT_ORM_RECEIVER_PATTERN,
        });
        assert!(out.is_empty());
    }
}
