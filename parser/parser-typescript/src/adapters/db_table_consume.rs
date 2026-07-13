//! DB-TABLE CONSUME extractor — projects the database tables a TS/JS tree reads or writes into
//! `db-table` io consumes, so the core cross-layer linker can surface `cross-layer/shared-db-table`
//! (the same table touched from 2+ distinct source trees). Join key: `(kind="db-table",
//! key="table:<accessor>")` — a plain string, same generic contract shape as the `http`/`trpc` kinds.
//!
//! ## What counts as a consume
//! A Prisma-style query call `getPrisma().<accessor>.<method>(...)`: a zero-arg client-getter call,
//! one plain-identifier model accessor, then any method call. Read OR write both count — `shared-db-table`
//! is about who *touches* a table, not the direction. The `getPrisma()` zero-arg anchor is the precision
//! guard (a bare `foo.bar.baz()` never matches), mirroring how `zzop_rules_schema::scan_store_map`
//! recognizes the same shape; the getter name is fixed to `zzop_parser_prisma`'s default convention.
//!
//! This is the FIRST producer feeding the (already generic) db-table io channel: the linker and
//! `shared-db-table` rule stay kind-agnostic, and this adapter supplies the facts. The bare-receiver form
//! (`prisma.<model>.<method>`, client an imported singleton) is a staged follow-up.
//!
//! ## Query call sites (`extract_query_call_sites`)
//! This file also hosts `extract_query_call_sites`, sharing `match_prisma_query_call`'s recognizer: it
//! restricts the same call shape to the 4 read-only query methods and emits `zzop_core::QueryCallSite`
//! facts, the per-file substrate `zzop_rules_schema::join`'s three schema x usage JOIN rules
//! (soft-delete-bypass / orderby-unindexed / enum-string-drift) scan. This replaces that crate's own
//! `<root>/src` filesystem re-walk (`zzop_rules_schema::join::scan_query_call_sites`, now removed) — the
//! facts instead travel through the fused per-file pass and cache, like `trpc_router_fragments`.

use swc_core::common::{SourceMap, SourceMapper, Span};
use swc_core::ecma::ast::{CallExpr, Callee, Expr, MemberProp};
use swc_core::ecma::visit::{Visit, VisitWith};
use zzop_core::{IoConsume, QueryCallSite};

/// The client-getter identifier the accessor chain must root at. Twin of
/// `zzop_parser_prisma::DEFAULT_PRISMA_CLIENT_GETTER_FN` (`"getPrisma"`) — kept as a local literal (not a
/// dependency on that crate) to avoid a parser-typescript -> parser-prisma edge for one string; `pub` so
/// `zzop_engine` can assert the two stay in sync (see its consistency-guard test) without either parser
/// depending on the other.
pub const PRISMA_CLIENT_GETTER: &str = "getPrisma";

/// The 4 Prisma query methods `extract_query_call_sites` restricts to — read-only call shapes the schema
/// x usage JOIN rules can reason about. Any other method (`create`/`update`/`aggregate`/...) is not a
/// query call site and is skipped. Authoritative: `QueryCallSite`s reach `zzop_rules_schema::join` already
/// filtered to this list, so that crate no longer keeps its own copy of this constant.
const QUERY_METHODS: [&str; 4] = ["findMany", "findFirst", "findUnique", "count"];

/// Extract db-table CONSUME entries from one file's raw source.
pub fn extract_db_table_consumes(rel: &str, text: &str) -> Vec<IoConsume> {
    // A test/spec file's table access isn't deployed DB coupling — skip before parsing, mirroring the
    // `is_test_file` skip other extractors and cross-layer rules already apply.
    if zzop_core::is_test_file(rel) {
        return Vec::new();
    }
    let Some((cm, module)) = crate::parse_with_cm(rel, text) else {
        return Vec::new();
    };
    let cm_ref: &SourceMap = &cm;
    let mut collector = DbTableCollector {
        cm: cm_ref,
        file: rel,
        out: Vec::new(),
    };
    module.visit_with(&mut collector);
    collector.out
}

struct DbTableCollector<'a> {
    cm: &'a SourceMap,
    file: &'a str,
    out: Vec<IoConsume>,
}

impl Visit for DbTableCollector<'_> {
    fn visit_call_expr(&mut self, call: &CallExpr) {
        if let Some(m) = match_prisma_query_call(call) {
            self.out.push(IoConsume {
                client: None,
                body: None,
                kind: "db-table".into(),
                key: Some(format!("table:{}", m.accessor)),
                file: self.file.into(),
                line: crate::line_of(self.cm, call.span.lo),
                raw: None,
                method: None,
            });
        }
        call.visit_children_with(self); // the inner `getPrisma()` call and any nested chains
    }
}

/// Extract `zzop_core::QueryCallSite` facts from one file's raw source — see this module's doc for how
/// these feed the schema x usage JOIN rules.
pub fn extract_query_call_sites(rel: &str, text: &str) -> Vec<QueryCallSite> {
    // A test/spec file's query call sites aren't real query surface for the schema x usage JOIN rules —
    // skip before parsing, same reasoning as `extract_db_table_consumes` above.
    if zzop_core::is_test_file(rel) {
        return Vec::new();
    }
    let Some((cm, module)) = crate::parse_with_cm(rel, text) else {
        return Vec::new();
    };
    let cm_ref: &SourceMap = &cm;
    let mut collector = QueryCallSiteCollector {
        cm: cm_ref,
        file: rel,
        out: Vec::new(),
    };
    module.visit_with(&mut collector);
    collector.out
}

struct QueryCallSiteCollector<'a> {
    cm: &'a SourceMap,
    file: &'a str,
    out: Vec<QueryCallSite>,
}

impl Visit for QueryCallSiteCollector<'_> {
    fn visit_call_expr(&mut self, call: &CallExpr) {
        if let Some(m) = match_prisma_query_call(call) {
            if QUERY_METHODS.contains(&m.method.as_str()) {
                // The balanced-paren argument span, `(...)` inclusive: from the end of the callee member
                // expression (`...<method>`) to the call's own end (right after the matching `)`).
                // `span_to_snippet` reads straight from the source map, so nested/multi-line arguments —
                // including their own nested parens/braces — come back verbatim. This carries the same
                // signal the removed regex scanner's `extract_balanced_parens` did; it is not byte-identical
                // (any whitespace between `<method>` and `(` is included here as a leading prefix, and the
                // three JOIN rules substring/`\b`-search this text so a leading space is inert).
                let arg_span = Span::new(m.callee_span.hi, call.span.hi);
                let call_text = self.cm.span_to_snippet(arg_span).unwrap_or_default();
                self.out.push(QueryCallSite {
                    model: capitalize(&m.accessor),
                    method: m.method,
                    file: self.file.into(),
                    line: crate::line_of(self.cm, call.span.lo),
                    call_text,
                });
            }
        }
        call.visit_children_with(self); // the inner `getPrisma()` call and any nested chains
    }
}

/// One `<getter>().<accessor>.<method>` match, shared by both collectors above.
struct PrismaQueryCall {
    /// The plain-identifier model accessor (`item` in `getPrisma().item.findMany(...)`).
    accessor: String,
    /// The method identifier (`findMany`, `create`, ...) — unfiltered; callers restrict as needed.
    method: String,
    /// The `<getter>().<accessor>.<method>` member expression's own span — `.hi` sits immediately before
    /// the call's own `(`, so `extract_query_call_sites` can slice the argument-list text from it.
    callee_span: Span,
}

/// Matches `<getter>().<accessor>.<method>(...)`, returning the accessor/method identifiers and the
/// callee member expression's span. `None` on any gate failure: a non-member callee, a computed segment,
/// a non-`getter` root, or a getter call with arguments.
fn match_prisma_query_call(call: &CallExpr) -> Option<PrismaQueryCall> {
    // `<obj>.<method>(...)`
    let Callee::Expr(callee) = &call.callee else {
        return None;
    };
    let Expr::Member(outer) = unwrap_expr(callee) else {
        return None;
    };
    let MemberProp::Ident(method) = &outer.prop else {
        return None; // computed `[...]` method — dynamic, skip honestly
    };
    // `<base>.<accessor>`
    let Expr::Member(mid) = unwrap_expr(&outer.obj) else {
        return None;
    };
    let MemberProp::Ident(accessor) = &mid.prop else {
        return None;
    };
    // `<getter>()` — a zero-arg call to the client getter identifier
    let Expr::Call(base) = unwrap_expr(&mid.obj) else {
        return None;
    };
    if !base.args.is_empty() {
        return None;
    }
    let Callee::Expr(base_callee) = &base.callee else {
        return None;
    };
    let Expr::Ident(id) = unwrap_expr(base_callee) else {
        return None;
    };
    if id.sym != PRISMA_CLIENT_GETTER {
        return None;
    }
    Some(PrismaQueryCall {
        accessor: accessor.sym.to_string(),
        method: method.sym.to_string(),
        callee_span: outer.span,
    })
}

/// Unwraps parens/`as`/`!`/`satisfies` wrappers to the inner expression. Local copy, mirroring the other
/// adapters in this module.
fn unwrap_expr(e: &Expr) -> &Expr {
    match e {
        Expr::Paren(p) => unwrap_expr(&p.expr),
        Expr::TsAs(a) => unwrap_expr(&a.expr),
        Expr::TsNonNull(n) => unwrap_expr(&n.expr),
        Expr::TsSatisfies(s) => unwrap_expr(&s.expr),
        other => other,
    }
}

/// First-char-uppercase (`item` -> `Item`) — mirrors `zzop_rules_schema::usage::capitalize` byte-for-byte;
/// duplicated locally to avoid a parser-typescript -> rules-schema dependency edge for one function.
fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(rel: &str, src: &str) -> Vec<String> {
        extract_db_table_consumes(rel, src)
            .into_iter()
            .map(|c| c.key.unwrap())
            .collect()
    }

    #[test]
    fn matches_read_and_write_methods() {
        let src = "async function f() {\n  await getPrisma().order.findMany({});\n  await getPrisma().user.create({});\n}\n";
        assert_eq!(keys("a.ts", src), vec!["table:order", "table:user"]);
    }

    #[test]
    fn ignores_non_getter_receiver() {
        // A bare `foo.bar.baz()` (no getPrisma() anchor) must not match.
        let src = "function f() {\n  foo.bar.findMany();\n  cache.get('x').then(() => 1);\n}\n";
        assert!(keys("a.ts", src).is_empty());
    }

    #[test]
    fn ignores_getter_with_args() {
        let src = "function f() {\n  getPrisma(tenant).order.findMany();\n}\n";
        assert!(keys("a.ts", src).is_empty());
    }

    #[test]
    fn then_chain_does_not_double_count() {
        let src = "function f() {\n  getPrisma().order.findMany().then(r => r);\n}\n";
        assert_eq!(keys("a.ts", src), vec!["table:order"]);
    }

    // --- extract_query_call_sites ---

    fn sites(rel: &str, src: &str) -> Vec<QueryCallSite> {
        extract_query_call_sites(rel, src)
    }

    #[test]
    fn finds_find_many_with_model_and_line() {
        let src = "export function list() {\n  return getPrisma().item.findMany({ where: { ownerId: 1 } });\n}\n";
        let out = sites("src/domains/item/repo.ts", src);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].model, "Item");
        assert_eq!(out[0].method, "findMany");
        assert_eq!(out[0].file, "src/domains/item/repo.ts");
        assert_eq!(out[0].line, 2);
        assert_eq!(out[0].call_text, "({ where: { ownerId: 1 } })");
    }

    #[test]
    fn captures_balanced_span_across_nested_braces() {
        let src = "export function list() {\n  return getPrisma().item.findMany({\n    where: { ownerId: 1, meta: { a: fn(1, 2) } },\n    orderBy: { name: 'asc' },\n  });\n}\n";
        let out = sites("a.ts", src);
        assert_eq!(out.len(), 1);
        assert!(out[0].call_text.contains("orderBy"));
        assert!(out[0].call_text.starts_with('('));
        assert!(out[0].call_text.ends_with(')'));
    }

    #[test]
    fn multiple_sites_same_file_correct_lines() {
        let src = "export function a() {\n  return getPrisma().item.findMany({});\n}\n\nexport function b() {\n  return getPrisma().item.count({});\n}\n";
        let out = sites("a.ts", src);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].line, 2);
        assert_eq!(out[0].method, "findMany");
        assert_eq!(out[1].line, 6);
        assert_eq!(out[1].method, "count");
    }

    #[test]
    fn ignores_non_query_methods() {
        // `create` is a db-table consume but not a query call site — the 4-method filter rejects it.
        let src = "export function f() { return getPrisma().item.create({ data: {} }); }\n";
        assert!(sites("a.ts", src).is_empty());
    }

    #[test]
    fn model_capitalization() {
        let src = "function f() { return getPrisma().userProfile.findFirst({}); }\n";
        let out = sites("a.ts", src);
        assert_eq!(out[0].model, "UserProfile");
    }

    #[test]
    fn then_chain_on_query_method_does_not_double_count() {
        let src = "function f() {\n  getPrisma().order.findMany().then(r => r);\n}\n";
        let out = sites("a.ts", src);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].method, "findMany");
    }

    #[test]
    fn test_files_are_skipped_by_both_extractors() {
        // A source that would otherwise match both extractors must yield nothing when the path is a
        // test/spec file — its DB access is not deployed coupling or real query surface.
        let src = "async function f() {\n  await getPrisma().order.findMany({});\n}\n";
        assert!(extract_db_table_consumes("src/order.test.ts", src).is_empty());
        assert!(extract_query_call_sites("src/order.test.ts", src).is_empty());
    }
}
