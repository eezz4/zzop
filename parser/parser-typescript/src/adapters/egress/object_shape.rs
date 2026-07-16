//! Static `ObjectLit` -> [`ConsumeBodyShape`] construction (`body-shape-v1`): depth-2-capped key
//! collection and per-level completeness, shared by every body style in `super::body_shape`.

use swc_core::ecma::ast::{Expr, ObjectLit, Prop, PropName, PropOrSpread};
use zzop_core::ConsumeBodyShape;

use super::unwrap_expr;

/// A property name statically readable from a `PropName` — `Ident`/`Str` only; `Computed` (and any
/// other future variant) returns `None`, which the caller treats as evidence the enclosing level is
/// incomplete (never guessed).
pub(super) fn prop_name_str(name: &PropName) -> Option<String> {
    match name {
        PropName::Ident(i) => Some(i.sym.to_string()),
        PropName::Str(s) => Some(s.value.as_str().unwrap_or_default().to_string()),
        _ => None,
    }
}

/// Walk ONE ObjectLit's own props (never recursing — the caller decides whether/how to descend),
/// pushing each witnessed key into `keys` (dotted under `prefix` when non-empty, bare otherwise).
/// Returns whether this level is fully static: `true` iff every prop is a plain `Prop::KeyValue`
/// (non-computed key) or `Prop::Shorthand` — a spread, computed key, getter, setter, method, or assign
/// prop makes this level incomplete (its sibling keys are still recorded; only the level's OWN
/// completeness is affected), per `ConsumeBodyShape::complete_at`'s contract.
fn collect_level(obj: &ObjectLit, prefix: &str, keys: &mut Vec<String>) -> bool {
    let mut complete = true;
    for prop in &obj.props {
        match prop {
            PropOrSpread::Spread(_) => complete = false,
            PropOrSpread::Prop(p) => match &**p {
                Prop::Shorthand(ident) => keys.push(dotted(prefix, &ident.sym)),
                Prop::KeyValue(kv) => match prop_name_str(&kv.key) {
                    Some(name) => keys.push(dotted(prefix, &name)),
                    None => complete = false, // PropName::Computed
                },
                Prop::Getter(_) | Prop::Setter(_) | Prop::Method(_) | Prop::Assign(_) => {
                    complete = false;
                }
            },
        }
    }
    complete
}

fn dotted(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}.{name}")
    }
}

/// Build the statically witnessed [`ConsumeBodyShape`] for a request-body `ObjectLit`, capped at depth 2
/// (the top level, plus one level under each top-level key whose value is itself an object literal —
/// see the type's doc). The top level's own completeness feeds `complete_at: [""]`; each depth-1
/// `KeyValue` whose value is an `ObjectLit` is walked exactly one more level (its keys recorded as
/// `"parent.child"`), and `"parent"` is added to `complete_at` too when THAT level is fully static. A
/// depth-2 nested object is recorded as a single key (its own dotted path) and never descended into —
/// `collect_level` only reads prop NAMES, never recurses on their values — so nothing past depth 2 is
/// ever produced, and a depth-2 path never enters `complete_at` (only `""` and depth-1 paths can).
pub(super) fn shape_from_object_lit(obj: &ObjectLit) -> ConsumeBodyShape {
    let mut keys = Vec::new();
    let top_complete = collect_level(obj, "", &mut keys);
    let mut complete_at = Vec::new();
    if top_complete {
        complete_at.push(String::new());
    }
    for prop in &obj.props {
        if let PropOrSpread::Prop(p) = prop {
            if let Prop::KeyValue(kv) = &**p {
                if let Some(name) = prop_name_str(&kv.key) {
                    if let Expr::Object(nested) = unwrap_expr(&kv.value) {
                        let nested_complete = collect_level(nested, &name, &mut keys);
                        if nested_complete {
                            complete_at.push(name);
                        }
                    }
                }
            }
        }
    }
    keys.sort();
    keys.dedup();
    complete_at.sort();
    complete_at.dedup();
    ConsumeBodyShape { keys, complete_at }
}

#[cfg(test)]
mod tests {
    use crate::adapters::egress::{extract_http_egress, files};

    #[test]
    fn axios_post_nested_object_literal_witnesses_two_level_shape() {
        let out = extract_http_egress(&files(&[(
            "a.tsx",
            "axios.post('/users', { user: { email, password } });",
        )]));
        assert_eq!(out.len(), 1);
        let body = out[0].body.as_ref().expect("body shape expected");
        assert_eq!(
            body.keys,
            vec!["user", "user.email", "user.password"]
                .into_iter()
                .map(String::from)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            body.complete_at,
            vec!["", "user"]
                .into_iter()
                .map(String::from)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn shorthand_body_prop_witnesses_the_key_without_descending() {
        let out = extract_http_egress(&files(&[("a.tsx", "axios.post('/users', { user });")]));
        let body = out[0].body.as_ref().unwrap();
        assert_eq!(body.keys, vec!["user".to_string()]);
        assert_eq!(body.complete_at, vec!["".to_string()]);
    }

    #[test]
    fn top_level_spread_marks_the_root_incomplete() {
        let out = extract_http_egress(&files(&[(
            "a.tsx",
            "axios.post('/users', { ...defaults, user });",
        )]));
        let body = out[0].body.as_ref().unwrap();
        assert_eq!(body.keys, vec!["user".to_string()]);
        assert!(
            !body.complete_at.contains(&"".to_string()),
            "spread at the top level must suppress the root's completeness: {body:?}"
        );
    }

    #[test]
    fn empty_object_literal_body_is_a_witnessed_empty_shape() {
        // An explicit `{}` IS evidence (a witnessed empty body), unlike a missing `args[1]` — `Some`
        // with empty `keys` but `complete_at: [""]`.
        let out = extract_http_egress(&files(&[("a.tsx", "axios.post('/users', {});")]));
        let body = out[0].body.as_ref().unwrap();
        assert!(body.keys.is_empty());
        assert_eq!(body.complete_at, vec!["".to_string()]);
    }

    #[test]
    fn depth_three_nesting_is_capped_at_depth_two() {
        let out = extract_http_egress(&files(&[(
            "a.tsx",
            "axios.post('/x', { a: { b: { c: 1 } } });",
        )]));
        let body = out[0].body.as_ref().unwrap();
        assert_eq!(body.keys, vec!["a".to_string(), "a.b".to_string()]);
        assert_eq!(body.complete_at, vec!["".to_string(), "a".to_string()]);
    }
}
