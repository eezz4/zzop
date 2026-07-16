//! The project-wide constant map (`const <Name> = {...}` object literals + string enums) and its two
//! resolution entry points: build-time folding and late cross-file re-resolution via
//! [`resolve_raw_path`].

use std::collections::HashMap;

use swc_core::ecma::ast::{
    Decl, Expr, Lit, ModuleDecl, ModuleItem, ObjectLit, Pat, Prop, PropName, PropOrSpread, Stmt,
    TsEnumMemberId,
};

use super::unwrap_expr;

/// One file's constant-map fragment: dotted constant access -> string value, from every top-level
/// `const <Name> = { ... }` object literal PLUS every top-level (incl. `export`) `enum` whose member
/// initializers are string literals (`RouteKey.Asset -> "assets"`) in this file's text alone. A member
/// with a numeric, implicit (auto-incrementing), or computed initializer is skipped — never guessed.
/// `build_const_map` folds this over every file; a caller with only one file in hand can merge fragments
/// later and re-resolve via [`resolve_raw_path`].
///
/// This map feeds TWO assemble-time consumers: [`resolve_raw_path`]'s late cross-file CONSUME
/// re-resolution, and (new) `zzop_engine::analyze::compose`'s controller-prefix PROVIDE resolution —
/// see `zzop_core::ControllerPrefixRouteFragment`'s doc for the `@Controller(RouteKey.Asset)` shape this
/// unblocks.
pub fn const_map_fragment(rel: &str, text: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let Some(module) = crate::parse_module(rel, text) else {
        return map;
    };
    for item in &module.body {
        let decl = match item {
            ModuleItem::Stmt(Stmt::Decl(d)) => Some(d),
            ModuleItem::ModuleDecl(ModuleDecl::ExportDecl(e)) => Some(&e.decl),
            _ => None,
        };
        if let Some(Decl::Var(v)) = decl {
            for d in &v.decls {
                if let (Pat::Ident(bi), Some(init)) = (&d.name, &d.init) {
                    // Object literals and (below) string enums only — dotted, namespaced keys
                    // (`RouteKey.Asset`). A bare `const path = "/x"` is deliberately NOT captured: this map
                    // is project-wide and scope-insensitive (last-write-wins), so a bare common name
                    // (`path`, `url`, `base`) could shadow a same-named function parameter and mis-key an
                    // unrelated `axios.get(path)` — a guess dressed as a visible fact. `str-concat-url-v1`
                    // resolves the visible LITERAL operands of a concat, not bare-const-prefix indirection.
                    if let Expr::Object(obj) = unwrap_expr(init) {
                        flatten(bi.id.sym.as_ref(), obj, &mut map);
                    }
                }
            }
        }
        if let Some(Decl::TsEnum(e)) = decl {
            let enum_name = e.id.sym.to_string();
            for member in &e.members {
                let member_name = match &member.id {
                    TsEnumMemberId::Ident(id) => id.sym.to_string(),
                    TsEnumMemberId::Str(s) => s.value.as_str().unwrap_or_default().to_string(),
                };
                // A numeric or implicit (no initializer at all — swc's auto-increment) member is
                // skipped: only a STRING literal initializer is a resolvable route-prefix constant.
                let Some(init) = &member.init else { continue };
                if let Expr::Lit(Lit::Str(s)) = unwrap_expr(init) {
                    map.insert(
                        format!("{enum_name}.{member_name}"),
                        s.value.as_str().unwrap_or_default().to_string(),
                    );
                }
            }
        }
    }
    map
}

/// Project-wide map of dotted constant access -> string value — the fold over [`const_map_fragment`]. A
/// key duplicated across two files resolves to whichever file's fragment is folded in last.
pub(super) fn build_const_map(files: &[(String, String)]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for (rel, text) in files {
        map.extend(const_map_fragment(rel, text));
    }
    map
}

/// Late re-resolution: resolves a consume's `raw` text against a (possibly wider) constant map if `raw`
/// is a plain dotted identifier chain present in `consts`; anything else (a call, a template, a bracket
/// access, a bare identifier with no dot) returns `None`.
pub fn resolve_raw_path(raw: &str, consts: &HashMap<String, String>) -> Option<String> {
    let trimmed = raw.trim();
    if !is_dotted_identifier_chain(trimmed) {
        return None;
    }
    consts.get(trimmed).cloned()
}

/// True for a plain dotted identifier chain with no calls/brackets/templates (`Foo.bar.baz`), false for
/// anything else including a single bare identifier with no dot (`+` requires at least one `.segment`).
fn is_dotted_identifier_chain(s: &str) -> bool {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"^[A-Za-z_$][\w$]*(\.[A-Za-z_$][\w$]*)+$").unwrap())
        .is_match(s)
}

fn flatten(prefix: &str, obj: &ObjectLit, map: &mut HashMap<String, String>) {
    for prop in &obj.props {
        let PropOrSpread::Prop(p) = prop else {
            continue;
        };
        let Prop::KeyValue(kv) = &**p else { continue };
        let name = match &kv.key {
            PropName::Ident(i) => i.sym.to_string(),
            PropName::Str(s) => s.value.as_str().unwrap_or_default().to_string(),
            _ => continue,
        };
        let key = format!("{prefix}.{name}");
        match unwrap_expr(&kv.value) {
            Expr::Lit(Lit::Str(s)) => {
                map.insert(key, s.value.as_str().unwrap_or_default().to_string());
            }
            Expr::Object(o) => flatten(&key, o, map),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{const_map_fragment, resolve_raw_path};

    // --- const_map_fragment / resolve_raw_path (late cross-file consume resolution substrate) ---

    #[test]
    fn const_map_fragment_flattens_one_files_nested_object_consts() {
        let frag = const_map_fragment(
            "protocol/ControlKey.ts",
            r#"export const ControlKey = { AUTHEN: { getUserInfo: "/authen/getUserInfo" } };"#,
        );
        assert_eq!(
            frag.get("ControlKey.AUTHEN.getUserInfo")
                .map(String::as_str),
            Some("/authen/getUserInfo")
        );
    }

    #[test]
    fn const_map_fragment_is_empty_for_a_file_with_no_top_level_object_const() {
        let frag = const_map_fragment("x.ts", "export const n = 1;\n");
        assert!(frag.is_empty());
    }

    // --- enum members joining the const map (controller-prefix-ref-v1) ---

    #[test]
    fn exported_string_enum_member_joins_the_const_map() {
        let frag = const_map_fragment(
            "enum.ts",
            "export enum RouteKey { Asset = 'assets', User = 'users' }\n",
        );
        assert_eq!(
            frag.get("RouteKey.Asset").map(String::as_str),
            Some("assets")
        );
        assert_eq!(frag.get("RouteKey.User").map(String::as_str), Some("users"));
    }

    #[test]
    fn non_exported_string_enum_member_joins_the_const_map_too() {
        let frag = const_map_fragment("enum.ts", "enum RouteKey { Asset = 'assets' }\n");
        assert_eq!(
            frag.get("RouteKey.Asset").map(String::as_str),
            Some("assets")
        );
    }

    #[test]
    fn numeric_enum_member_is_skipped_not_guessed() {
        let frag = const_map_fragment("enum.ts", "enum Level { Low = 0, High = 1 }\n");
        assert!(
            frag.is_empty(),
            "numeric initializers must never join the const map: {frag:?}"
        );
    }

    #[test]
    fn implicit_auto_increment_enum_member_is_skipped_not_guessed() {
        let frag = const_map_fragment("enum.ts", "enum Level { Low, High }\n");
        assert!(
            frag.is_empty(),
            "a member with no initializer at all must never guess a value: {frag:?}"
        );
    }

    #[test]
    fn mixed_string_and_numeric_enum_members_only_joins_the_string_ones() {
        let frag = const_map_fragment(
            "enum.ts",
            "export enum Mixed { Path = 'x', Count = 1, Auto }\n",
        );
        assert_eq!(frag.len(), 1);
        assert_eq!(frag.get("Mixed.Path").map(String::as_str), Some("x"));
    }

    #[test]
    fn resolve_raw_path_hits_a_dotted_chain_present_in_the_map() {
        let mut consts = HashMap::new();
        consts.insert(
            "ControlKey.AUTHEN.getUserInfo".to_string(),
            "/authen/getUserInfo".to_string(),
        );
        assert_eq!(
            resolve_raw_path("ControlKey.AUTHEN.getUserInfo", &consts).as_deref(),
            Some("/authen/getUserInfo")
        );
    }

    #[test]
    fn resolve_raw_path_misses_a_dotted_chain_absent_from_the_map() {
        let consts = HashMap::new();
        assert_eq!(
            resolve_raw_path("ControlKey.AUTHEN.getUserInfo", &consts),
            None
        );
    }

    #[test]
    fn resolve_raw_path_rejects_a_call_expression() {
        let mut consts = HashMap::new();
        consts.insert("buildUrl".to_string(), "/should/not/match".to_string());
        assert_eq!(resolve_raw_path("buildUrl(x)", &consts), None);
    }

    #[test]
    fn resolve_raw_path_rejects_a_template_literal() {
        let consts = HashMap::new();
        assert_eq!(resolve_raw_path("`/api/${id}`", &consts), None);
    }

    #[test]
    fn resolve_raw_path_rejects_a_bare_identifier_with_no_dot() {
        let mut consts = HashMap::new();
        // Even if a bare name happened to be a map key (it never is — `flatten` only inserts dotted
        // keys), an identifier with no `.` must still be rejected: the regex requires one `.segment`.
        consts.insert("ControlKey".to_string(), "/should/not/match".to_string());
        assert_eq!(resolve_raw_path("ControlKey", &consts), None);
    }
}
