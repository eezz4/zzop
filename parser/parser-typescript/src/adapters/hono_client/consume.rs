//! Call-chain recognition + CONSUME emission for `hono_client` — see the parent module doc for
//! the chain-recognition spec.

use std::collections::HashSet;

use swc_core::common::SourceMap;
use swc_core::ecma::ast::{CallExpr, Callee, Expr, Lit, MemberProp};
use swc_core::ecma::visit::{Visit, VisitWith};
use zzop_core::{http_consume_interface_key, IoConsume};

use super::scan::unwrap_expr;

pub(super) struct ConsumeCollector<'a> {
    pub(super) cm: &'a SourceMap,
    pub(super) file: &'a str,
    pub(super) ident_receivers: &'a HashSet<String>,
    pub(super) this_field_receivers: &'a HashSet<String>,
    pub(super) base: &'a Option<String>,
    pub(super) out: Vec<IoConsume>,
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
                        client: None,
                        body: None,
                        kind: "http".into(),
                        key: Some(http_consume_interface_key(verb, &path)),
                        file: self.file.into(),
                        line: crate::line_of(self.cm, call.span.lo),
                        raw: None,
                        method: None,
                        retry_configured: None,
                    }
                }
                None => {
                    let chain_text = if segs.is_empty() {
                        root
                    } else {
                        format!("{root}.{}", segs.join("."))
                    };
                    IoConsume {
                        client: None,
                        body: None,
                        kind: "http".into(),
                        key: None,
                        file: self.file.into(),
                        line: crate::line_of(self.cm, call.span.lo),
                        raw: Some(format!("{chain_text} {terminal_sym}")),
                        method: Some(verb.to_string()),
                        retry_configured: None,
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
