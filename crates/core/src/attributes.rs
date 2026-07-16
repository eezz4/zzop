//! Generic entity-attribute annotation channel — the open-vocab injection surface for cross-cutting facts
//! attached to code entities (a route, a symbol, a file, a path scope) that per-file extraction cannot see
//! (auth guards, model usage, rate limits, …). A producer/adapter emits `Attribute`s; a rule consumes them
//! BY KEY. This module is VOCAB-FREE: it never branches on what `key` means ("auth-guarded", …) — exactly
//! as the cross-layer `(kind, key)` join is vocab-free. One generic channel instead of a bespoke typed
//! field per fact class (`is_entry`, `bound_models`, …) — applying the kernel-agnostic principle to the
//! injection contract itself. First consumer: `mutating-route-no-auth` (injected `auth-guarded` evidence
//! for middleware the call-graph can't see).

use serde::{Deserialize, Serialize};

/// A JSON value read off an attribute is "truthy" — `null`/`false`/`0`/`""` mean absent/off; anything
/// else means present/on. Lets a bare `true` mean on and an explicit `false` override a broader scope.
pub fn attr_is_truthy(v: &serde_json::Value) -> bool {
    match v {
        serde_json::Value::Null => false,
        serde_json::Value::Bool(b) => *b,
        serde_json::Value::Number(n) => n.as_f64().is_none_or(|f| f != 0.0),
        serde_json::Value::String(s) => !s.is_empty(),
        _ => true,
    }
}

/// What an [`Attribute`] attaches to. Externally tagged, so an adapter emits `{ "ioKey": { … } }`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EntityRef {
    /// A file, by its repo-relative forward-slash path.
    File { path: String },
    /// A symbol by name, optionally disambiguated by declaring file.
    Symbol {
        name: String,
        #[serde(default)]
        file: Option<String>,
    },
    /// A join key — the same `(kind, key)` coordinate the cross-layer join uses (e.g. kind `"http"`,
    /// key `"POST /api/users"`).
    IoKey { kind: String, key: String },
    /// A path-prefix scope — applies to every http route whose path is under `prefix` (longest match
    /// wins), the shape a router-level middleware guards.
    PathScope { prefix: String },
}

/// One cross-cutting annotation: `key -> value` attached to `target`. `key` is producer/rule vocabulary;
/// this type is agnostic to it. `value` is open JSON — most consumers use `true` or a scalar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Attribute {
    pub target: EntityRef,
    pub key: String,
    pub value: serde_json::Value,
}

/// Assembled attribute lookup — every injected [`Attribute`] across an analysis, queryable by a rule
/// without the engine ever branching on an attribute key. Built at assemble time from the injected
/// `attributes` channels; the engine hands a reference to the consuming rules.
#[derive(Debug, Default, Clone)]
pub struct AttributeStore {
    attrs: Vec<Attribute>,
}

impl AttributeStore {
    pub fn from_attrs(attrs: Vec<Attribute>) -> Self {
        Self { attrs }
    }

    /// Build the store from every Mode-B adapter overlay's per-file `attributes`, flattened tree-wide. The
    /// single source of "attributes come from overlays" — both the callgraph and schema passes call this.
    pub fn from_overlays(overlays: &[crate::NormalizedEnvelope]) -> Self {
        Self::from_parts(Vec::new(), overlays)
    }

    /// Build the store from a NATIVE-producer attribute list plus every Mode-B adapter overlay's
    /// per-file `attributes` — the merge point for the two independent attribute sources: a native
    /// parser's own composed judgments (e.g. a recognized Express middleware guard, riding
    /// `zzop_core::RouterMountEntry`'s fragment composition) and a user-supplied overlay's injected
    /// facts. Every lookup method here (`route_attr`'s exact `IoKey` branch, `symbol_attr`/`file_attr`'s
    /// `find_map`) returns the FIRST match it finds — a pre-existing, unchanged invariant of this store —
    /// so overlay entries are inserted BEFORE native ones: for the same `(target, key)` pair, an overlay
    /// attribute wins over a native one. A user's explicit injection is taken as a deliberate override of
    /// the native parser's own judgment, not merely additive to it.
    pub fn from_parts(native: Vec<Attribute>, overlays: &[crate::NormalizedEnvelope]) -> Self {
        let mut attrs: Vec<Attribute> = overlays
            .iter()
            .flat_map(|env| env.files.iter())
            .flat_map(|file| file.attributes.iter().cloned())
            .collect();
        attrs.extend(native);
        Self::from_attrs(attrs)
    }

    pub fn is_empty(&self) -> bool {
        self.attrs.is_empty()
    }

    /// Look up `attr_key` for a symbol by `name` (optionally disambiguated by declaring `file`). Matches an
    /// `EntityRef::Symbol` whose `name` equals `name` and whose own `file` is either absent (name-only
    /// binding) or equals `file` when both are given. Returns the first match's value. VOCAB-FREE.
    pub fn symbol_attr(
        &self,
        name: &str,
        file: Option<&str>,
        attr_key: &str,
    ) -> Option<&serde_json::Value> {
        self.attrs.iter().find_map(|a| {
            if a.key != attr_key {
                return None;
            }
            match &a.target {
                EntityRef::Symbol { name: n, file: f }
                    if n == name && (f.is_none() || file.is_none() || f.as_deref() == file) =>
                {
                    Some(&a.value)
                }
                _ => None,
            }
        })
    }

    /// Look up `attr_key` for a file by repo-relative forward-slash `path`. Matches an `EntityRef::File`
    /// whose `path` equals `path`. Returns the first match's value. VOCAB-FREE.
    pub fn file_attr(&self, path: &str, attr_key: &str) -> Option<&serde_json::Value> {
        self.attrs.iter().find_map(|a| match &a.target {
            EntityRef::File { path: p } if p == path && a.key == attr_key => Some(&a.value),
            _ => None,
        })
    }

    /// Look up `attr_key` for an http-style route identified by `(kind, key)` where `key` is
    /// `"METHOD /path"`. An exact [`EntityRef::IoKey`] target wins outright (route-level override); else
    /// the longest [`EntityRef::PathScope`] whose prefix covers the route's path on segment boundaries.
    /// Returns the raw value if present. VOCAB-FREE: `attr_key` is the caller's, never this module's.
    pub fn route_attr(&self, kind: &str, key: &str, attr_key: &str) -> Option<&serde_json::Value> {
        let path = key.split_once(' ').map(|(_, p)| p).unwrap_or(key);
        let mut best: Option<(usize, &serde_json::Value)> = None; // (matched prefix len, value)
        for a in &self.attrs {
            if a.key != attr_key {
                continue;
            }
            match &a.target {
                EntityRef::IoKey { kind: k, key: kk } if k == kind && kk == key => {
                    return Some(&a.value); // exact route match is most specific — wins outright
                }
                EntityRef::PathScope { prefix } if kind == "http" && path_under(path, prefix) => {
                    let len = prefix.trim_end_matches('/').len();
                    if best.is_none_or(|(l, _)| len > l) {
                        best = Some((len, &a.value));
                    }
                }
                _ => {}
            }
        }
        best.map(|(_, v)| v)
    }
}

/// `path` is under `prefix` on segment boundaries: equal to it (trailing slashes ignored), or starts with
/// `prefix` followed by `/`. `"/admin"` covers `"/admin"` and `"/admin/users"`, not `"/administrators"`.
fn path_under(path: &str, prefix: &str) -> bool {
    let p = prefix.trim_end_matches('/');
    path == p
        || path
            .strip_prefix(p)
            .is_some_and(|rest| rest.starts_with('/'))
}

#[cfg(test)]
mod tests;
