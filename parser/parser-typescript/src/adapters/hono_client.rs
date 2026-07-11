//! hono/client typed-RPC CONSUME extractor — projects the client-side calls a TS/JS tree makes
//! through Hono's `hc<AppType>()` typed proxy client, so the core cross-layer linker can join each
//! call to its backend Hono route. Unlike `trpc_consume` (whose dotted procedure path IS the join
//! key), a hono/client call chain only names the ROUTE TAIL, so this module also resolves the BASE
//! PATH the client was constructed with, once per file — kind `"http"`, keyed via
//! `http_consume_interface_key` like `egress`'s FE HTTP-call extractor.
//!
//! Only files that import `hc` from a specifier containing `hono/client` (case-insensitively) are
//! scanned — a bare `.signout.$post()`-shaped member call is otherwise far too generic to key.
//!
//! ## Base-path resolution (from the FIRST `hc(...)`/`hc<T>(...)` call in the file)
//! A string literal starting with `/` is the base verbatim; a full external URL contributes its path
//! part only. A template literal concatenates quasis with each interpolation as `{}`, then takes the
//! substring from the first `/` (`None` if that still contains `{}`). An identifier or one-level
//! member access (e.g. `options.baseUrl`) is traced ONE hop: if the call sits inside a class
//! instantiated elsewhere with an object literal carrying the same property name
//! (`new SomeClient({ baseUrl: <expr> })`), the same rules apply to `<expr>`; any other shape leaves
//! the base unresolved.
//!
//! An unresolved base does NOT skip the file — every recognized call chain is still extracted, just
//! emitted UNRESOLVED (`key: None`, `raw: Some("<chain> $verb")`, `method: Some(VERB)`), the same
//! honest "seen but unkeyed" shape `egress`'s dynamic-URL consumes use.
//!
//! ## Call-chain recognition
//! A "hc-derived receiver" is a local binding via `const client = hc<T>(...)` or
//! `this.<field> = hc<T>(...)` inside a class — a class wrapping Hono's client factory, the shape a
//! generated API client SDK commonly uses. A call chain is a member-access sequence rooted at a
//! recognized receiver, each link a plain `.ident` or a bracket string literal (a Hono `:param`
//! segment kept verbatim), ending in a terminal `.$get()`/`.$post()`/`.$put()`/`.$patch()`/`.$delete()`
//! call. A `.param(...)`/`.query(...)` link anywhere is transparently skipped; any other embedded
//! call, or a non-literal computed segment, aborts the whole chain.

use std::collections::HashSet;

use swc_core::common::{BytePos, SourceMap};
use swc_core::ecma::ast::{
    AssignExpr, AssignTarget, CallExpr, Callee, ClassDecl, Expr, Lit, MemberProp, NewExpr, Pat,
    Prop, PropName, PropOrSpread, SimpleAssignTarget, Tpl, VarDeclarator,
};
use swc_core::ecma::visit::{Visit, VisitWith};
use zzop_core::{http_consume_interface_key, IoConsume};

/// Extract hono/client typed-RPC CONSUME entries from one file.
pub fn extract_hono_client_consumes(rel: &str, text: &str) -> Vec<IoConsume> {
    let mut hc_idents: HashSet<String> = HashSet::new();
    for (local, binding) in crate::parse_imports(rel, text) {
        if binding.original == "hc"
            && binding
                .specifier
                .to_ascii_lowercase()
                .contains("hono/client")
        {
            hc_idents.insert(local);
        }
    }
    if hc_idents.is_empty() {
        return Vec::new();
    }

    let Some((cm, module)) = crate::parse_with_cm(rel, text) else {
        return Vec::new();
    };

    let mut scanner = ClientScanner {
        hc_idents: &hc_idents,
        class_stack: Vec::new(),
        ident_receivers: HashSet::new(),
        this_field_receivers: HashSet::new(),
        first_call: None,
        hc_call_count: 0,
    };
    module.visit_with(&mut scanner);

    // Multi-client guard: with 2+ `hc()` calls in one file there is no single base to attribute a
    // chain to — keying every receiver with the FIRST call's base would silently mis-key the
    // others, so the base is treated as non-static and every chain emits UNRESOLVED instead.
    let base: Option<String> = if scanner.hc_call_count > 1 {
        None
    } else {
        scanner.first_call.and_then(|fc| {
            fc.resolved.or_else(|| {
                let (prop_name, class_name) = fc.trace?;
                let mut finder = NewExprPropFinder {
                    class_name: &class_name,
                    prop_name: &prop_name,
                    result: None,
                };
                module.visit_with(&mut finder);
                finder.result
            })
        })
    };

    let cm_ref: &SourceMap = &cm;
    let mut collector = ConsumeCollector {
        cm: cm_ref,
        file: rel,
        ident_receivers: &scanner.ident_receivers,
        this_field_receivers: &scanner.this_field_receivers,
        base: &base,
        out: Vec::new(),
    };
    module.visit_with(&mut collector);
    collector.out
}

/// Info about the FIRST `hc(...)`/`hc<T>(...)` call in the file, used to derive the base path.
struct FirstCall {
    span_lo: BytePos,
    /// Directly resolved from a literal/template argument.
    resolved: Option<String>,
    /// `(one-hop property name, enclosing class name)` for the `new ClassName({ prop: <expr> })` trace.
    trace: Option<(String, String)>,
}

/// Single pass collecting: hc-derived receivers (both routes), and the file's first hc() call info.
struct ClientScanner<'a> {
    hc_idents: &'a HashSet<String>,
    class_stack: Vec<String>,
    ident_receivers: HashSet<String>,
    this_field_receivers: HashSet<String>,
    first_call: Option<FirstCall>,
    /// Total `hc(...)` calls seen — 2+ means the file has no single attributable base.
    hc_call_count: u32,
}

impl Visit for ClientScanner<'_> {
    fn visit_class_decl(&mut self, n: &ClassDecl) {
        self.class_stack.push(n.ident.sym.to_string());
        n.visit_children_with(self);
        self.class_stack.pop();
    }

    fn visit_var_declarator(&mut self, d: &VarDeclarator) {
        if let (Pat::Ident(bi), Some(init)) = (&d.name, &d.init) {
            if is_hc_call(unwrap_expr(init), self.hc_idents) {
                self.ident_receivers.insert(bi.id.sym.to_string());
            }
        }
        d.visit_children_with(self);
    }

    fn visit_assign_expr(&mut self, n: &AssignExpr) {
        if let AssignTarget::Simple(SimpleAssignTarget::Member(m)) = &n.left {
            if let (Expr::This(_), MemberProp::Ident(field)) = (unwrap_expr(&m.obj), &m.prop) {
                if is_hc_call(unwrap_expr(&n.right), self.hc_idents) {
                    self.this_field_receivers.insert(field.sym.to_string());
                }
            }
        }
        n.visit_children_with(self);
    }

    fn visit_call_expr(&mut self, call: &CallExpr) {
        if is_hc_callee(call, self.hc_idents) {
            self.hc_call_count += 1;
            let lo = call.span.lo();
            let earlier = self
                .first_call
                .as_ref()
                .map(|fc| lo < fc.span_lo)
                .unwrap_or(true);
            if earlier {
                if let Some(arg) = call.args.first() {
                    let resolved = resolve_literal_or_template(&arg.expr);
                    let trace = if resolved.is_none() {
                        one_hop_target(&arg.expr).zip(self.class_stack.last().cloned())
                    } else {
                        None
                    };
                    self.first_call = Some(FirstCall {
                        span_lo: lo,
                        resolved,
                        trace,
                    });
                }
            }
        }
        call.visit_children_with(self); // recurse into nested calls
    }
}

/// True when `expr` (already unwrapped) is a call whose callee is a plain identifier in `hc_idents`.
fn is_hc_call(expr: &Expr, hc_idents: &HashSet<String>) -> bool {
    matches!(expr, Expr::Call(call) if is_hc_callee(call, hc_idents))
}

fn is_hc_callee(call: &CallExpr, hc_idents: &HashSet<String>) -> bool {
    let Callee::Expr(callee) = &call.callee else {
        return false;
    };
    matches!(unwrap_expr(callee), Expr::Ident(id) if hc_idents.contains(id.sym.as_ref()))
}

/// The property name to look up on a same-file `new <EnclosingClass>({ <prop>: <expr> })` call.
fn one_hop_target(expr: &Expr) -> Option<String> {
    match unwrap_expr(expr) {
        Expr::Ident(id) => Some(id.sym.to_string()),
        Expr::Member(m) => match &m.prop {
            MemberProp::Ident(name) => Some(name.sym.to_string()),
            _ => None,
        },
        _ => None,
    }
}

/// Finds the first `new <class_name>({ ..., <prop_name>: <expr>, ... })` and resolves `<expr>` via
/// [`resolve_literal_or_template`] — the "one hop" the base-path trace is allowed to take.
struct NewExprPropFinder<'a> {
    class_name: &'a str,
    prop_name: &'a str,
    result: Option<String>,
}

impl Visit for NewExprPropFinder<'_> {
    fn visit_new_expr(&mut self, n: &NewExpr) {
        let is_target_class =
            matches!(unwrap_expr(&n.callee), Expr::Ident(id) if id.sym == self.class_name);
        if self.result.is_none() && is_target_class {
            if let Some(Expr::Object(obj)) = n
                .args
                .as_ref()
                .and_then(|args| args.first())
                .map(|a| unwrap_expr(&a.expr))
            {
                for prop in &obj.props {
                    let PropOrSpread::Prop(p) = prop else {
                        continue;
                    };
                    let Prop::KeyValue(kv) = &**p else {
                        continue;
                    };
                    let name = match &kv.key {
                        PropName::Ident(i) => i.sym.to_string(),
                        PropName::Str(s) => s.value.as_str().unwrap_or_default().to_string(),
                        _ => continue,
                    };
                    if name == self.prop_name {
                        self.result = resolve_literal_or_template(&kv.value);
                    }
                }
            }
        }
        n.visit_children_with(self);
    }
}

/// Resolves a string-literal or template-literal base-path expression, `None` for any other shape.
fn resolve_literal_or_template(expr: &Expr) -> Option<String> {
    match unwrap_expr(expr) {
        Expr::Lit(Lit::Str(s)) => resolve_str_literal(s.value.as_str().unwrap_or_default()),
        Expr::Tpl(t) => resolve_template(t),
        _ => None,
    }
}

/// `/api/auth` -> itself; `https://host/api/auth` -> `/api/auth`; anything else (no leading slash,
/// no scheme) -> `None`, never guessed.
fn resolve_str_literal(v: &str) -> Option<String> {
    if v.starts_with('/') {
        Some(v.to_string())
    } else if let Some(idx) = v.to_ascii_lowercase().find("://") {
        let rest = &v[idx + 3..];
        rest.find('/').map(|i| rest[i..].to_string())
    } else {
        None
    }
}

/// Concatenates quasis with each interpolation as a `{}` placeholder, then takes the substring
/// starting at the first `/`. `None` if that substring still contains `{}` or has no `/` at all.
fn resolve_template(t: &Tpl) -> Option<String> {
    let mut s = String::new();
    for (i, q) in t.quasis.iter().enumerate() {
        s.push_str(
            q.cooked
                .as_ref()
                .and_then(|a| a.as_str())
                .unwrap_or_default(),
        );
        if i < t.exprs.len() {
            s.push_str("{}");
        }
    }
    // Scheme strip — same `://` handling as `resolve_str_literal`: a full URL written as a backtick
    // string must yield the PATH part, not fold the host into the key.
    let idx = if let Some(scheme_idx) = s.find("://") {
        let after_scheme = scheme_idx + 3;
        s[after_scheme..].find('/').map(|i| after_scheme + i)?
    } else {
        s.find('/')?
    };
    let candidate = &s[idx..];
    if candidate.contains("{}") {
        None
    } else {
        Some(candidate.to_string())
    }
}

struct ConsumeCollector<'a> {
    cm: &'a SourceMap,
    file: &'a str,
    ident_receivers: &'a HashSet<String>,
    this_field_receivers: &'a HashSet<String>,
    base: &'a Option<String>,
    out: Vec<IoConsume>,
}

impl Visit for ConsumeCollector<'_> {
    fn visit_call_expr(&mut self, call: &CallExpr) {
        if let Some((verb, terminal_sym, root, segs)) =
            match_hono_call(call, self.ident_receivers, self.this_field_receivers)
        {
            let consume = match self.base {
                Some(base) => {
                    let path = if segs.is_empty() {
                        base.clone()
                    } else {
                        format!("{base}/{}", segs.join("/"))
                    };
                    IoConsume {
                        kind: "http".into(),
                        key: Some(http_consume_interface_key(verb, &path)),
                        file: self.file.into(),
                        line: crate::line_of(self.cm, call.span.lo),
                        raw: None,
                        method: None,
                    }
                }
                None => {
                    let chain_text = if segs.is_empty() {
                        root
                    } else {
                        format!("{root}.{}", segs.join("."))
                    };
                    IoConsume {
                        kind: "http".into(),
                        key: None,
                        file: self.file.into(),
                        line: crate::line_of(self.cm, call.span.lo),
                        raw: Some(format!("{chain_text} {terminal_sym}")),
                        method: Some(verb.to_string()),
                    }
                }
            };
            self.out.push(consume);
        }
        call.visit_children_with(self); // recurse into nested calls
    }
}

/// Matches `<hc-derived receiver>. ... .<$verb>(...)`; `None` when the terminal isn't a recognized
/// `$verb` or [`collect_chain`] rejects the chain.
fn match_hono_call(
    call: &CallExpr,
    ident_receivers: &HashSet<String>,
    this_field_receivers: &HashSet<String>,
) -> Option<(&'static str, String, String, Vec<String>)> {
    let Callee::Expr(callee) = &call.callee else {
        return None;
    };
    let Expr::Member(outer) = unwrap_expr(callee) else {
        return None;
    };
    let MemberProp::Ident(terminal) = &outer.prop else {
        return None;
    };
    let verb = terminal_verb(terminal.sym.as_ref())?;
    let (root, segs) = collect_chain(&outer.obj, ident_receivers, this_field_receivers)?;
    Some((verb, terminal.sym.to_string(), root, segs))
}

/// Walks a member/call-chain expression down to its receiver root, collecting path segments (root and
/// terminal exclusive). A computed segment that isn't a string literal, an unrecognized embedded
/// call, or an unknown root abandons the whole chain (`None`).
fn collect_chain(
    expr: &Expr,
    ident_receivers: &HashSet<String>,
    this_field_receivers: &HashSet<String>,
) -> Option<(String, Vec<String>)> {
    match unwrap_expr(expr) {
        Expr::Ident(id) => {
            let name = id.sym.to_string();
            if ident_receivers.contains(&name) {
                Some((name, Vec::new()))
            } else {
                None
            }
        }
        Expr::Member(m) => {
            if matches!(unwrap_expr(&m.obj), Expr::This(_)) {
                let MemberProp::Ident(field) = &m.prop else {
                    return None;
                };
                let name = field.sym.to_string();
                return if this_field_receivers.contains(&name) {
                    Some((format!("this.{name}"), Vec::new()))
                } else {
                    None
                };
            }
            let seg = match &m.prop {
                MemberProp::Ident(name) => name.sym.to_string(),
                MemberProp::Computed(c) => match unwrap_expr(&c.expr) {
                    Expr::Lit(Lit::Str(s)) => s.value.as_str().unwrap_or_default().to_string(),
                    _ => return None,
                },
                MemberProp::PrivateName(_) => return None,
            };
            let (root, mut segs) = collect_chain(&m.obj, ident_receivers, this_field_receivers)?;
            segs.push(seg);
            Some((root, segs))
        }
        Expr::Call(c) => {
            let Callee::Expr(callee) = &c.callee else {
                return None;
            };
            let Expr::Member(cm_) = unwrap_expr(callee) else {
                return None;
            };
            let MemberProp::Ident(name) = &cm_.prop else {
                return None;
            };
            if matches!(name.sym.as_ref(), "param" | "query") {
                collect_chain(&cm_.obj, ident_receivers, this_field_receivers)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// The hono/client terminal -> CONSUME verb mapping: `$` + a lowercase spelling of a
/// `zzop_core::HTTP_KEY_VERBS` verb (T1: the verb set lives in core; the `$` prefix is this
/// vocabulary's own spelling rule).
fn terminal_verb(name: &str) -> Option<&'static str> {
    let bare = name.strip_prefix('$')?;
    zzop_core::HTTP_KEY_VERBS
        .iter()
        .find(|v| v.to_ascii_lowercase() == bare)
        .copied()
}

/// Strip wrappers between an expression and its real value: `... as const`, `(...)`, `... satisfies T`, `...!`.
fn unwrap_expr(e: &Expr) -> &Expr {
    let mut n = e;
    loop {
        n = match n {
            Expr::TsAs(a) => &a.expr,
            Expr::TsConstAssertion(c) => &c.expr,
            Expr::Paren(p) => &p.expr,
            Expr::TsSatisfies(s) => &s.expr,
            Expr::TsNonNull(nn) => &nn.expr,
            other => return other,
        };
    }
}

#[cfg(test)]
mod tests {
    //! Coverage for `extract_hono_client_consumes`: a class-field receiver, a direct `hc(...)` literal
    //! base, bracket segments, non-static bases, and the file-gate no-import case.
    use super::*;

    fn keys(out: &[IoConsume]) -> Vec<Option<String>> {
        out.iter().map(|c| c.key.clone()).collect()
    }

    #[test]
    fn class_field_receiver_auth_client_shape_end_to_end() {
        let src = r#"
import { hc } from 'hono/client';
type AuthClientType = ReturnType<typeof hc<AuthAppType>>;
export class AuthClient {
  public client: AuthClientType;
  constructor(options: { baseUrl: string }) {
    this.client = hc<AuthAppType>(options.baseUrl);
  }
  public async signOut() {
    await this.client.signout.$post();
  }
  public async signOutAllSessions() {
    await this.client['signout-all'].$post();
  }
  public async getSession() {
    const r = await this.client['session-json'].$get();
  }
}
export const authClient = new AuthClient({ baseUrl: `${NEXT_PUBLIC_WEBAPP_URL()}/api/auth` });
"#;
        let out = extract_hono_client_consumes("client/index.ts", src);
        assert_eq!(
            keys(&out),
            vec![
                Some("POST /api/auth/signout".to_string()),
                Some("POST /api/auth/signout-all".to_string()),
                Some("GET /api/auth/session-json".to_string()),
            ]
        );
        assert!(out
            .iter()
            .all(|c| c.kind == "http" && c.raw.is_none() && c.method.is_none()));
        assert_eq!(out[0].line, 10);
        assert_eq!(out[1].line, 13);
        assert_eq!(out[2].line, 16);
    }

    #[test]
    fn direct_string_literal_base_with_dotted_chain() {
        let out = extract_hono_client_consumes(
            "a.ts",
            "import { hc } from 'hono/client';\nconst client = hc<T>('/api/auth');\nclient.two.factor.$post();",
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key.as_deref(), Some("POST /api/auth/two/factor"));
        assert_eq!(out[0].line, 3);
    }

    #[test]
    fn bracket_segment_is_a_literal_path_segment() {
        let out = extract_hono_client_consumes(
            "b.ts",
            "import { hc } from 'hono/client';\nconst client = hc('/api/auth');\nclient['signout-all'].$post();",
        );
        assert_eq!(out[0].key.as_deref(), Some("POST /api/auth/signout-all"));
    }

    #[test]
    fn non_static_base_with_no_same_file_trace_is_unresolved() {
        let out = extract_hono_client_consumes(
            "c.ts",
            "import { hc } from 'hono/client';\nconst client = hc(someVar);\nclient.two.factor.$post();",
        );
        assert_eq!(out.len(), 1);
        assert!(out[0].key.is_none());
        assert_eq!(out[0].raw.as_deref(), Some("client.two.factor $post"));
        assert_eq!(out[0].method.as_deref(), Some("POST"));
    }

    #[test]
    fn template_base_with_interpolation_inside_the_path_is_unresolved() {
        let out = extract_hono_client_consumes(
            "d.ts",
            "import { hc } from 'hono/client';\nconst client = hc(`/api/${v}/auth`);\nclient.two.$get();",
        );
        assert_eq!(out.len(), 1);
        assert!(out[0].key.is_none());
        assert_eq!(out[0].raw.as_deref(), Some("client.two $get"));
        assert_eq!(out[0].method.as_deref(), Some("GET"));
    }

    #[test]
    fn no_hono_client_import_yields_nothing_even_with_dollar_verb_calls() {
        let out = extract_hono_client_consumes(
            "e.ts",
            "const client = hc('/api/auth');\nclient.two.factor.$get();",
        );
        assert!(out.is_empty());
    }

    #[test]
    fn bare_param_query_helper_calls_are_skipped_not_path_segments() {
        let out = extract_hono_client_consumes(
            "f.ts",
            "import { hc } from 'hono/client';\nconst client = hc('/api/posts');\nclient[':id'].param({ id: '123' }).$get();",
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key.as_deref(), Some("GET /api/posts/{}"));
    }
}
