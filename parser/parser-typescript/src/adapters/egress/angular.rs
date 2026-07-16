//! Angular dependency-injected `HttpClient` recognition (`angular-httpclient-v1`) — see the module
//! doc on [`super`] for the exact evidence gate.

use std::collections::HashSet;

use swc_core::ecma::ast::{
    CallExpr, Callee, ClassProp, Constructor, Expr, MemberProp, Module, ModuleDecl, ModuleItem,
    ParamOrTsParamProp, Pat, PropName, TsEntityName, TsParamPropParam, TsType, TsTypeAnn, VarDecl,
    VarDeclKind,
};
use swc_core::ecma::visit::{Visit, VisitWith};

use super::body_shape::BodyStyle;
use super::matchers::{is_http_method, HttpCall};

/// Angular HttpClient call matcher (`angular-httpclient-v1`) — `this.<name>.<verb>(url, ...)` or
/// `<name>.<verb>(url, ...)` where `<name>` is a proven HttpClient receiver (see module doc). Sibling to
/// [`super::matchers::match_http_call`], never called when `receivers` is empty (the file didn't import
/// `@angular/common/http`, or nothing in it resolved as an HttpClient receiver).
pub(super) fn match_angular_http_call<'a>(
    call: &'a CallExpr,
    receivers: &HashSet<String>,
) -> Option<HttpCall<'a>> {
    if receivers.is_empty() {
        return None;
    }
    let arg = &*call.args.first()?.expr;
    let Callee::Expr(callee) = &call.callee else {
        return None;
    };
    let Expr::Member(outer) = &**callee else {
        return None;
    };
    let MemberProp::Ident(verb_ident) = &outer.prop else {
        return None;
    };
    let verb = verb_ident.sym.to_string();
    if !is_http_method(&verb) {
        return None;
    }
    let receiver_name = match &*outer.obj {
        // `this.<name>.<verb>(...)`
        Expr::Member(inner) => {
            if !matches!(&*inner.obj, Expr::This(_)) {
                return None;
            }
            let MemberProp::Ident(name_ident) = &inner.prop else {
                return None;
            };
            name_ident.sym.to_string()
        }
        // `<name>.<verb>(...)` — a field-inject or local-const-inject receiver referenced bare.
        Expr::Ident(id) => id.sym.to_string(),
        _ => return None,
    };
    if !receivers.contains(&receiver_name) {
        return None;
    }
    Some(HttpCall {
        methods: vec![verb],
        arg,
        body_style: BodyStyle::DirectArg,
        client: "angular",
    })
}

/// True when the file imports (any specifier) from `@angular/common/http` — the hard per-file evidence
/// gate for [`match_angular_http_call`]; see module doc (`angular-httpclient-v1`).
fn has_angular_http_client_import(module: &Module) -> bool {
    module.body.iter().any(|item| {
        matches!(
            item,
            ModuleItem::ModuleDecl(ModuleDecl::Import(imp))
                if imp.src.value.as_str() == Some("@angular/common/http")
        )
    })
}

/// This file's set of proven Angular HttpClient receiver names — empty unless the file imports
/// `@angular/common/http` (module doc). Three shapes contribute: a constructor parameter property typed
/// `HttpClient`, a class property typed `HttpClient` or initialized with `inject(HttpClient)`, and a
/// top-level or function-local `const`/`let` initialized with `inject(HttpClient)`. Resolution walks the
/// WHOLE tree (not just top-level), so a nested method-local `inject(HttpClient)` const is found too.
pub(super) fn angular_http_client_receivers(module: &Module) -> HashSet<String> {
    if !has_angular_http_client_import(module) {
        return HashSet::new();
    }
    let mut c = HttpClientReceiverCollector {
        names: HashSet::new(),
    };
    module.visit_with(&mut c);
    c.names
}

struct HttpClientReceiverCollector {
    names: HashSet<String>,
}

impl Visit for HttpClientReceiverCollector {
    fn visit_constructor(&mut self, n: &Constructor) {
        for p in &n.params {
            if let ParamOrTsParamProp::TsParamProp(tpp) = p {
                if let TsParamPropParam::Ident(bi) = &tpp.param {
                    if is_http_client_type(bi.type_ann.as_deref()) {
                        self.names.insert(bi.id.sym.to_string());
                    }
                }
            }
        }
        n.visit_children_with(self);
    }

    fn visit_class_prop(&mut self, n: &ClassProp) {
        if let PropName::Ident(key) = &n.key {
            let is_http_client = is_http_client_type(n.type_ann.as_deref())
                || n.value.as_deref().is_some_and(is_inject_http_client_call);
            if is_http_client {
                self.names.insert(key.sym.to_string());
            }
        }
        n.visit_children_with(self);
    }

    fn visit_var_decl(&mut self, n: &VarDecl) {
        if matches!(n.kind, VarDeclKind::Const | VarDeclKind::Let) {
            for d in &n.decls {
                if let (Pat::Ident(bi), Some(init)) = (&d.name, d.init.as_deref()) {
                    if is_inject_http_client_call(init) {
                        self.names.insert(bi.id.sym.to_string());
                    }
                }
            }
        }
        n.visit_children_with(self);
    }
}

/// `: HttpClient` type annotation — a single-identifier `TsTypeRef` named exactly `HttpClient`.
fn is_http_client_type(ann: Option<&TsTypeAnn>) -> bool {
    let Some(ann) = ann else { return false };
    matches!(&*ann.type_ann, TsType::TsTypeRef(tr) if matches!(&tr.type_name, TsEntityName::Ident(id) if id.sym == "HttpClient"))
}

/// `inject(HttpClient)` — callee identifier `inject`, exactly one argument, that argument the bare
/// identifier `HttpClient`.
fn is_inject_http_client_call(e: &Expr) -> bool {
    let Expr::Call(call) = e else { return false };
    let Callee::Expr(callee) = &call.callee else {
        return false;
    };
    let Expr::Ident(id) = &**callee else {
        return false;
    };
    id.sym == "inject"
        && call.args.len() == 1
        && matches!(&*call.args[0].expr, Expr::Ident(arg) if arg.sym == "HttpClient")
}

#[cfg(test)]
mod tests {
    use crate::adapters::egress::{clients, extract_http_egress, files, keys};

    #[test]
    fn angular_constructor_param_property_http_client_is_recognized() {
        let out = extract_http_egress(&files(&[(
            "article.service.ts",
            concat!(
                "import { HttpClient } from '@angular/common/http';\n",
                "export class ArticleService {\n",
                "  constructor(private readonly http: HttpClient) {}\n",
                "  getArticles() {\n",
                "    this.http.get<{a: string}>('/articles');\n",
                "    this.http.post('/users', {});\n",
                "    this.http.delete(`/articles/${slug}`);\n",
                "  }\n",
                "}\n",
            ),
        )]));
        assert_eq!(
            keys(&out),
            vec![
                Some("GET /articles".to_string()),
                Some("POST /users".to_string()),
                Some("DELETE /articles/{}".to_string()),
            ]
        );
    }

    #[test]
    fn angular_field_inject_http_client_is_recognized() {
        let out = extract_http_egress(&files(&[(
            "article.service.ts",
            concat!(
                "import { HttpClient } from '@angular/common/http';\n",
                "import { inject } from '@angular/core';\n",
                "export class ArticleService {\n",
                "  private http = inject(HttpClient);\n",
                "  getArticles() {\n",
                "    return this.http.get('/articles');\n",
                "  }\n",
                "}\n",
            ),
        )]));
        assert_eq!(keys(&out), vec![Some("GET /articles".to_string())]);
    }

    #[test]
    fn angular_local_const_inject_http_client_is_recognized() {
        let out = extract_http_egress(&files(&[(
            "a.ts",
            concat!(
                "import { HttpClient } from '@angular/common/http';\n",
                "import { inject } from '@angular/core';\n",
                "function useArticles() {\n",
                "  const http = inject(HttpClient);\n",
                "  return http.get('/x');\n",
                "}\n",
            ),
        )]));
        assert_eq!(keys(&out), vec![Some("GET /x".to_string())]);
    }

    #[test]
    fn angular_shape_without_the_import_is_not_recognized() {
        // Same shape, no `@angular/common/http` import anywhere in the file — never guessed.
        let out = extract_http_egress(&files(&[(
            "a.ts",
            "export class ArticleService {\n  constructor(private readonly http) {}\n  x() { this.http.get('/x'); }\n}\n",
        )]));
        assert!(out.is_empty());
    }

    #[test]
    fn angular_gated_file_but_non_http_client_typed_receiver_is_not_recognized() {
        let out = extract_http_egress(&files(&[(
            "a.ts",
            concat!(
                "import { HttpClient } from '@angular/common/http';\n",
                "export class AuthService {\n",
                "  constructor(private readonly http: HttpClient, private readonly jwtService: JwtService) {}\n",
                "  x() {\n",
                "    this.jwtService.get('/x');\n",
                "  }\n",
                "}\n",
            ),
        )]));
        assert!(out.is_empty());
    }

    #[test]
    fn angular_http_client_call_is_tagged_angular() {
        let out = extract_http_egress(&files(&[(
            "article.service.ts",
            concat!(
                "import { HttpClient } from '@angular/common/http';\n",
                "export class ArticleService {\n",
                "  constructor(private readonly http: HttpClient) {}\n",
                "  getArticles() {\n",
                "    this.http.get('/articles');\n",
                "  }\n",
                "}\n",
            ),
        )]));
        assert_eq!(clients(&out), vec![Some("angular".to_string())]);
    }
}
