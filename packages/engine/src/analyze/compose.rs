//! Cross-file / fragment composition — the "fragment now, compose later" passes over data the fused
//! per-file pass already collected (no second parse): late cross-file constant re-resolution for `http`
//! CONSUMEs, tRPC router-fragment composition into `trpc` PROVIDEs, code-registered router-mount
//! composition into `http` PROVIDEs, wrapper-consume joins, controller-prefix route-fragment
//! resolution into `http` PROVIDEs, the NestJS global-prefix apply/strip, and the axios `baseURL`
//! path-prefix apply/strip (the CONSUME-side counterpart of the global-prefix seam).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use zzop_core::{http_consume_interface_key, http_interface_key, IoConsume, IoProvide};

/// Deterministically merges every file's own constant-map fragment into one project-wide map — the
/// shared substrate both [`late_resolve_cross_file_consumes`] (CONSUME re-resolution) and
/// `compose_controller_prefix_provides` (PROVIDE resolution) resolve against, so a `RouteKey.Asset`
/// enum member and an `axios.get(ControlKey.X)` constant are both looked up in exactly the same map.
///
/// `fragments` is sorted by `rel`, then folded first-writer-wins: a constant key duplicated across two
/// files always resolves to the lexicographically smallest file's value, independent of
/// `HashMap`/rayon iteration order. Takes `&[...]` (not by value) so a caller can compute this merged
/// map before separately consuming the same `fragments` `Vec` elsewhere (e.g.
/// `late_resolve_cross_file_consumes`, which still owns its own copy of the merge for its own callers/
/// tests).
pub(crate) fn merge_const_map_fragments(
    fragments: &[(String, HashMap<String, String>)],
) -> HashMap<String, String> {
    let mut sorted: Vec<&(String, HashMap<String, String>)> = fragments.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    let mut merged: BTreeMap<String, String> = BTreeMap::new();
    for (_, fragment) in sorted {
        for (key, value) in fragment {
            merged.entry(key.clone()).or_insert_with(|| value.clone());
        }
    }
    // `BTreeMap` above exists only so the merge loop itself is deterministic; callers have no ordering
    // requirement of their own, so this returns a plain `HashMap`.
    merged.into_iter().collect()
}

/// Late cross-file constant re-resolution — closes the gap `crate::io`'s module doc documents as the "v1
/// fusion tradeoff": a one-file-slice HTTP egress scan cannot resolve a constant imported from another
/// file, so it emits `IoConsume { key: None, raw: Some(<dotted expr text>), method: Some(<METHOD>) }`
/// instead of guessing. This function fixes that up AFTER every file's own constant-map fragment has
/// been collected, using only data the fused per-file pass already produced — no second parse.
///
/// **Deterministic merge**: delegates to [`merge_const_map_fragments`] — see its own doc.
///
/// **Re-resolution**: every consume with `key: None` whose `raw`/`method` are both `Some` is looked up
/// via `zzop_parser_typescript::resolve_raw_path`; a hit sets `key` to the normalized join key and
/// deliberately keeps `raw` as provenance (this consume was only resolvable via the project-wide
/// constant merge, not from its own file alone). A miss leaves the consume exactly as unresolved as
/// before — this function only ever turns an unresolved consume INTO a resolved one, never the reverse.
///
/// Must run before `io_consumes` is frozen into `MinimalIr::io` — every whole-tree native rule that
/// reads `io_consumes` directly must see the resolved key, not the raw one.
pub(crate) fn late_resolve_cross_file_consumes(
    fragments: Vec<(String, HashMap<String, String>)>,
    io_consumes: &mut [IoConsume],
) {
    let consts = merge_const_map_fragments(&fragments);
    for consume in io_consumes.iter_mut() {
        if consume.key.is_some() {
            continue;
        }
        let (Some(raw), Some(method)) = (consume.raw.as_deref(), consume.method.as_deref()) else {
            continue;
        };
        if let Some(path) = zzop_parser_typescript::resolve_raw_path(raw, &consts) {
            // A leading `/` is an internal route (normalized key); an absolute `http(s)://` URL keeps
            // the verbatim host-carrying key so `link_cross_layer_io`'s `"://"` gate still routes it
            // to the `external` bucket; a base-relative path literal (`users/login` — the axios
            // `baseURL` idiom) keys as its root-normalized form, mirroring the egress extractor's own
            // gating; anything else stays unresolved. Deliberately NO base-carrier head-drop bucket
            // here (unlike `consume_key_for`'s 4-bucket dispatch): `resolve_raw_path` only accepts
            // dotted-const chains whose resolved values are source string literals, so a `{}`-headed
            // assembled variant can never reach this mirror — the omission is structural, not drift.
            if path.starts_with('/') {
                consume.key = Some(http_consume_interface_key(method, &path));
            } else if zzop_parser_typescript::is_external_url(&path) {
                consume.key = Some(format!("{method} {path}"));
            } else if let Some(rooted) = zzop_parser_typescript::base_relative_path(&path) {
                consume.key = Some(http_consume_interface_key(method, &rooted));
            }
        }
    }
}

/// Resolves `ControllerPrefixRouteFragment`s (`controller-prefix-ref-v1` — a `@Controller(RouteKey.Asset)`
/// dotted member-expression prefix, deferred by `zzop_parser_typescript::extract_controller_prefix_route_fragments`
/// because a single file can't see where `RouteKey` is declared) into whole-tree `http` `IoProvide`s.
///
/// `consts` is the SAME project-wide merged constant map [`late_resolve_cross_file_consumes`] uses
/// (built by [`merge_const_map_fragments`], which now also folds string-valued `enum` members — see
/// `zzop_parser_typescript::const_map_fragment`'s doc) — a caller computes it once and passes it to both.
///
/// A `prefix_ref` present in `consts` resolves exactly like `extract_controller_provides`'s own literal
/// path: `"{prefix}/{path}"` joined and normalized via `http_interface_key`. A `prefix_ref` ABSENT from
/// `consts` never guesses — instead one `warnings` entry is pushed per distinct `(file, prefix_ref)`
/// pair, naming the ref, the file, and how many routes were dropped, and no provide is emitted for any
/// of that controller's fragments.
///
/// ## Placement (load-bearing — see `zzop_engine::analyze::mod`'s call site)
/// Must run BEFORE `apply_and_strip_global_prefix`: a NestJS tree can compose BOTH a `RouteKey.Asset` ->
/// `assets` prefix resolution here AND a `setGlobalPrefix('api')` rewrite there on the very same route
/// (`GET /api/assets/{}`) — this function's output has to already be in `io_provides` for the global-
/// prefix seam to see and prepend it, same requirement every other per-file-composed provide has at
/// that seam.
pub(crate) fn compose_controller_prefix_provides(
    fragments: Vec<(String, Vec<zzop_core::ControllerPrefixRouteFragment>)>,
    consts: &HashMap<String, String>,
    warnings: &mut Vec<String>,
) -> Vec<IoProvide> {
    let mut fragments = fragments;
    fragments.sort_by(|a, b| a.0.cmp(&b.0));

    let mut out = Vec::new();
    // `(file, prefix_ref) -> count of routes dropped` — one aggregated warning per distinct pair rather
    // than one per route, so a 5-route controller with an unresolvable prefix produces one honest line,
    // not five.
    let mut unresolved: BTreeMap<(String, String), u32> = BTreeMap::new();

    for (file, frags) in &fragments {
        for frag in frags {
            match consts.get(&frag.prefix_ref) {
                Some(prefix) => {
                    let full_path = format!("{prefix}/{}", frag.path);
                    out.push(IoProvide {
                        // Carried through so a prefix-ref route's composed `IoProvide` keeps the same
                        // body evidence a literal-prefix route gets directly (`ControllerPrefixRouteFragment`
                        // doc) — `resolve_provide_body_refs` (below) resolves its `dto_ref` afterward
                        // exactly like any other provide's.
                        body: frag.body.clone(),
                        kind: "http".to_string(),
                        key: http_interface_key(&frag.verb, &full_path),
                        file: file.clone(),
                        line: frag.line,
                        symbol: frag.symbol.clone(),
                    });
                }
                None => {
                    *unresolved
                        .entry((file.clone(), frag.prefix_ref.clone()))
                        .or_insert(0) += 1;
                }
            }
        }
    }

    for ((file, prefix_ref), count) in unresolved {
        let route_word = if count == 1 { "route" } else { "routes" };
        warnings.push(format!(
            "could not resolve controller prefix `{prefix_ref}` ({file}) to a literal — its {count} {route_word} are not projected; the prefix constant may live in an unanalyzed file"
        ));
    }

    out.sort_by(|a, b| {
        a.kind
            .cmp(&b.kind)
            .then_with(|| a.key.cmp(&b.key))
            .then_with(|| a.file.cmp(&b.file))
            .then_with(|| a.line.cmp(&b.line))
    });
    out
}

/// Resolves `IoProvide::body`'s `dto_ref` (`body-shape-v1`) against the tree-wide merged class-shape map
/// (`zzop_core::ClassShapeFragment`) — the assemble-time counterpart of
/// [`compose_controller_prefix_provides`]'s constant-ref resolution, but for request-body DTO classes: a
/// `@Body() dto: CreateUserDto` provide only names the DTO class by its identifier; the class declaration
/// usually lives in another file, so a single-file scan can't resolve it (see `ProvideBodyShape`'s own doc).
///
/// ## Merge (never guess)
/// `class_shapes` is sorted by file path first for a deterministic scan order (mirrors
/// [`merge_const_map_fragments`]'s determinism rationale), then folded into one `name -> ClassShapeFragment`
/// map:
/// - A name declared identically (same `fields` + `complete`) in one file, or repeated identically across
///   2+ files, resolves normally.
/// - A name declared with CONFLICTING shapes (different `fields` or `complete`) across 2+ files is
///   POISONED — it resolves to nothing for every provide referencing it, and ONE aggregated warning names
///   the class and every file that declared it, rather than guessing which declaration is authoritative.
///
/// ## Provide resolution
/// Every provide whose `body` is `Some(shape)` with `shape.dto_ref == Some(name)`:
/// - `name` resolved (found, not poisoned): `fields`/`complete` are copied from the merged shape and
///   `dto_ref` is cleared to `None` — fully resolved, matching `ProvideBodyShape`'s own doc.
/// - `name` absent from the merge, or poisoned: the WHOLE `body` is dropped to `None` (never guessed,
///   same policy as an unresolved `prefix_ref`) — one aggregated warning per distinct `(file, dto_ref)`
///   pair, naming the ref, the file, and how many provides in that file lost their body contract, mirroring
///   [`compose_controller_prefix_provides`]'s aggregation style.
///
/// Must run AFTER every provide-composition pass (`compose_controller_prefix_provides`, the global-prefix
/// seam, `compose_trpc_provides`, `compose_router_mount_provides`, file-convention routes) so a
/// prefix-ref-composed provide's body also gets resolved here — see `zzop_engine::analyze::mod`'s call site.
pub(crate) fn resolve_provide_body_refs(
    io_provides: &mut [IoProvide],
    class_shapes: Vec<(String, Vec<zzop_core::ClassShapeFragment>)>,
    warnings: &mut Vec<String>,
) {
    let mut class_shapes = class_shapes;
    class_shapes.sort_by(|a, b| a.0.cmp(&b.0));

    let mut shapes_by_name: BTreeMap<String, zzop_core::ClassShapeFragment> = BTreeMap::new();
    let mut files_by_name: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut poisoned: HashSet<String> = HashSet::new();

    for (file, frags) in &class_shapes {
        for frag in frags {
            files_by_name
                .entry(frag.name.clone())
                .or_default()
                .insert(file.clone());
            match shapes_by_name.get(&frag.name) {
                None => {
                    shapes_by_name.insert(frag.name.clone(), frag.clone());
                }
                Some(existing) => {
                    if existing.fields != frag.fields || existing.complete != frag.complete {
                        poisoned.insert(frag.name.clone());
                    }
                }
            }
        }
    }

    // Only disclose a poisoned name some provide's `dto_ref` actually references: class-shape
    // fragments are emitted for EVERY class declaration, so same-name/different-shape non-DTO
    // classes (`Config`, `Options`, feature-local types) are common and legitimate — warning on an
    // unreferenced collision would disclose a drop that never happened (a phantom disclosure, the
    // same stance `unmatched_suppression_warnings` codifies).
    let referenced: HashSet<&str> = io_provides
        .iter()
        .filter_map(|p| p.body.as_ref().and_then(|b| b.dto_ref.as_deref()))
        .collect();
    let mut poisoned_names: Vec<&String> = poisoned
        .iter()
        .filter(|n| referenced.contains(n.as_str()))
        .collect();
    poisoned_names.sort();
    for name in poisoned_names {
        let files: Vec<&str> = files_by_name[name].iter().map(String::as_str).collect();
        warnings.push(format!(
            "class `{name}` is declared with conflicting field shapes across {} files ({}) — request-body \
             resolution for `{name}` is dropped, never guessed",
            files.len(),
            files.join(", ")
        ));
    }

    // One aggregated warning per (file, dto_ref) whose ref could not be resolved — count of provides
    // dropped, mirroring `compose_controller_prefix_provides`'s aggregation style.
    let mut unresolved: BTreeMap<(String, String), u32> = BTreeMap::new();

    for provide in io_provides.iter_mut() {
        let Some(dto_ref) = provide.body.as_ref().and_then(|b| b.dto_ref.clone()) else {
            continue;
        };
        if poisoned.contains(&dto_ref) {
            provide.body = None;
            *unresolved
                .entry((provide.file.clone(), dto_ref))
                .or_insert(0) += 1;
            continue;
        }
        match shapes_by_name.get(&dto_ref) {
            Some(frag) => {
                if let Some(shape) = provide.body.as_mut() {
                    shape.fields = frag.fields.clone();
                    shape.complete = frag.complete;
                    shape.dto_ref = None;
                }
            }
            None => {
                provide.body = None;
                *unresolved
                    .entry((provide.file.clone(), dto_ref))
                    .or_insert(0) += 1;
            }
        }
    }

    for ((file, dto_ref), count) in unresolved {
        let provide_word = if count == 1 { "provide" } else { "provides" };
        warnings.push(format!(
            "could not resolve request-body DTO `{dto_ref}` ({file}) to a known class shape — its {count} \
             {provide_word} keep no body contract; the DTO class may live in an unanalyzed file"
        ));
    }
}

/// NestJS `app.setGlobalPrefix(...)` apply + strip — see `zzop_parser_typescript::adapters::global_prefix`'s
/// module doc for why this rides the `provides` channel as a `nest-global-prefix` sentinel instead of a
/// dedicated field.
///
/// ## Placement (load-bearing — scope correctness)
/// This MUST run at exactly one seam: right after the per-file IO collection loop (which folds every
/// file's `IoFacts.provides` — Nest-controller `http` provides plus the `nest-global-prefix` sentinels —
/// into `io_provides`), and BEFORE the whole-tree provide producers that append OTHER `http` provides:
/// the Java Spring pass (`run_java_provides_project_pass`), Hono/Express router-mount composition
/// (`compose_router_mount_provides`), and file-convention routes (`file_routes`). Those routes carry
/// their own full path (a Next.js `GET /api/foo` is already complete) and must NOT be prefixed — running
/// later would double-prefix them (`GET /api/api/foo`). At this seam `io_provides` holds ONLY per-file
/// provides, so the rewrite is inherently scoped to Nest controllers.
///
/// ## Behavior
/// - Exactly one distinct sentinel value, non-empty after trimming surrounding `/`: every `http`
///   provide's key is rewritten to prepend that prefix (one clean `/` at the seam).
/// - Exactly one distinct sentinel value that normalizes to empty (`''` or `'/'`): a no-op rewrite — an
///   empty global prefix means no prefix — but the sentinel is still stripped.
/// - More than one distinct value: nothing is rewritten — a `warnings` entry is pushed instead (honest
///   degrade over guessing which one is real).
/// - Zero sentinel values: a no-op.
///
/// In every case, every `nest-global-prefix` provide is removed from `io_provides` — the sentinel must
/// never reach output or the cross-layer join.
pub(super) fn apply_and_strip_global_prefix(
    io_provides: &mut Vec<IoProvide>,
    warnings: &mut Vec<String>,
) {
    // Bound to the parser's exported const (not a local literal) so a rename on the emit side
    // cannot silently desynchronize the strip side — a leaked sentinel would reach output.
    const SENTINEL_KIND: &str = zzop_parser_typescript::NEST_GLOBAL_PREFIX_KIND;

    let mut prefixes: Vec<String> = io_provides
        .iter()
        .filter(|p| p.kind == SENTINEL_KIND)
        .map(|p| p.key.clone())
        .collect();
    prefixes.sort();
    prefixes.dedup();

    match prefixes.as_slice() {
        [] => {}
        [prefix] => {
            // Trim surrounding slashes on both ends: `setGlobalPrefix('api')`, `'/api'`, and `'api/'`
            // all normalize to `api`; `''` and `'/'` normalize to empty (an empty prefix means no
            // prefix — skip the rewrite entirely, but still strip the sentinel below).
            let prefix = prefix.trim_matches('/');
            if !prefix.is_empty() {
                for p in io_provides.iter_mut() {
                    if p.kind == "http" {
                        p.key = prepend_global_prefix(&p.key, prefix);
                    }
                }
            }
        }
        _ => {
            warnings.push(format!(
                "multiple setGlobalPrefix values found: [{}]; skipping global-prefix rewrite",
                prefixes.join(", ")
            ));
        }
    }

    io_provides.retain(|p| p.kind != SENTINEL_KIND);
}

/// Prepends a global-route prefix (already leading-slash-stripped by the caller) onto an `http` provide
/// key of the shape `"VERB /path"`, producing exactly one `/` at the seam: `("GET /articles", "api")` ->
/// `"GET /api/articles"`; `("GET /", "api")` -> `"GET /api"`. A key with no space (never produced by
/// `http_interface_key`, but handled defensively) is returned unchanged.
fn prepend_global_prefix(key: &str, prefix: &str) -> String {
    let Some((verb, path)) = key.split_once(' ') else {
        return key.to_string();
    };
    let rest = path.strip_prefix('/').unwrap_or(path);
    if rest.is_empty() {
        format!("{verb} /{prefix}")
    } else {
        format!("{verb} /{prefix}/{rest}")
    }
}

/// Composes every file's tRPC router fragment into whole-tree `IoProvide`s — the assembly-time
/// counterpart of `late_resolve_cross_file_consumes` for the `trpc` kind, except here the cross-file
/// join produces brand-new PROVIDEs directly rather than re-keying an already-emitted CONSUME: a leaf's
/// full dotted route path is often only knowable once every file's fragment is assembled together.
///
/// `resolve` is `(specifier, from_file) -> Option<target_rel>` — the caller passes a closure over
/// `zzop_parser_typescript::resolve_file_with_workspace` (the same resolver `assemble` uses for TS
/// dep-graph edges) so this function itself stays a pure, filesystem-free composition — easy to unit
/// test with a hand-built resolver map.
///
/// ## Resolution
/// Fragments are indexed by `(rel, name)`. Each `TrpcRouterEntry::Ref` is resolved to a target fragment
/// key: `specifier: Some(s)` -> `resolve(s, rel)` gives the target file, then `(target_rel, ident)`;
/// `specifier: None` -> same-file, `(rel, ident)`. A `Ref` whose specifier does not resolve, or whose
/// resolved key names no known fragment, is skipped — honest absence, never fabricated.
///
/// ## Roots and composition
/// A fragment is a ROOT when no resolved `Ref` anywhere in the corpus targets it — composition starts
/// from every root (BTree-ordered for determinism) and walks each fragment's entries depth-first:
/// `Nested` appends its `key` to the current dotted path; `Ref` with a non-empty `key` appends `key`
/// then recurses into the target fragment; `Ref` with an empty `key` (a `mergeRouters(...)` argument)
/// splices the target's entries in at the current path, adding no segment; `Leaf` emits one `IoProvide`
/// (`file`/`line` from the leaf's own originating fragment, which after a `Ref` hop is the target
/// fragment's file, not the file containing the `Ref`). An `ancestry` stack guards against a cyclic
/// `Ref` chain — a fragment already on the stack is skipped rather than recursed into again.
///
/// Deduped on `(kind, key, file, line)` and sorted to match the ordering `assemble` applies to every
/// other `IoProvide` before freezing `MinimalIr::io`.
pub(crate) fn compose_trpc_provides(
    fragments: Vec<(String, Vec<zzop_core::TrpcRouterFragment>)>,
    resolve: impl Fn(&str, &str) -> Option<String>,
) -> Vec<IoProvide> {
    use zzop_core::{TrpcRouterEntry, TrpcRouterFragment};

    let mut by_key: HashMap<(String, String), &TrpcRouterFragment> = HashMap::new();
    for (rel, frags) in &fragments {
        for frag in frags {
            by_key.insert((rel.clone(), frag.name.clone()), frag);
        }
    }

    // Resolves one `Ref`'s target fragment key, honoring the `specifier: Some -> resolve` / `specifier:
    // None -> same-file` split — shared by both the "which fragments are targeted" pass below and the
    // actual composition walk.
    let ref_target = |origin_rel: &str, ident: &str, specifier: &Option<String>| {
        let target_rel = match specifier {
            Some(s) => resolve(s, origin_rel)?,
            None => origin_rel.to_string(),
        };
        let key = (target_rel, ident.to_string());
        by_key.contains_key(&key).then_some(key)
    };

    // Every fragment key targeted by at least one resolved `Ref`, anywhere — plus, conservatively, every
    // ident ANY `Ref` names (resolved or not): a fragment whose name some mount references is intended
    // as a sub-router somewhere, so promoting it to a root when the mount's specifier failed to resolve
    // would emit its leaves under a truncated path (e.g. bare `createLicense` instead of
    // `viewer.admin.createLicense`) — a mis-keyed provide. Skipping it entirely is the honest-absence
    // choice; the cost is missing a genuinely independent root that merely shares its name with some ref
    // ident — rare, and an under- rather than over-report.
    let mut targeted: HashSet<(String, String)> = HashSet::new();
    let mut ref_named_idents: HashSet<String> = HashSet::new();
    fn collect_targeted(
        entries: &[TrpcRouterEntry],
        origin_rel: &str,
        ref_target: &impl Fn(&str, &str, &Option<String>) -> Option<(String, String)>,
        targeted: &mut HashSet<(String, String)>,
        ref_named_idents: &mut HashSet<String>,
    ) {
        for entry in entries {
            match entry {
                TrpcRouterEntry::Ref {
                    ident, specifier, ..
                } => {
                    ref_named_idents.insert(ident.clone());
                    if let Some(key) = ref_target(origin_rel, ident, specifier) {
                        targeted.insert(key);
                    }
                }
                TrpcRouterEntry::Nested { entries, .. } => {
                    collect_targeted(entries, origin_rel, ref_target, targeted, ref_named_idents);
                }
                TrpcRouterEntry::Leaf { .. } => {}
            }
        }
    }
    for (rel, frags) in &fragments {
        for frag in frags {
            collect_targeted(
                &frag.entries,
                rel,
                &ref_target,
                &mut targeted,
                &mut ref_named_idents,
            );
        }
    }

    let mut roots: Vec<(String, String)> = by_key
        .keys()
        .filter(|k| !targeted.contains(*k) && !ref_named_idents.contains(&k.1))
        .cloned()
        .collect();
    roots.sort();

    #[allow(clippy::too_many_arguments)]
    fn compose_entries(
        entries: &[TrpcRouterEntry],
        origin_rel: &str,
        path: &[String],
        by_key: &HashMap<(String, String), &TrpcRouterFragment>,
        ref_target: &impl Fn(&str, &str, &Option<String>) -> Option<(String, String)>,
        ancestry: &mut Vec<(String, String)>,
        out: &mut Vec<IoProvide>,
    ) {
        for entry in entries {
            match entry {
                TrpcRouterEntry::Leaf { key, verb, line } => {
                    let full_path = if path.is_empty() {
                        key.clone()
                    } else {
                        format!("{}.{key}", path.join("."))
                    };
                    out.push(IoProvide {
                        body: None,
                        kind: "trpc".to_string(),
                        key: format!("{verb} {full_path}"),
                        file: origin_rel.to_string(),
                        line: *line,
                        symbol: None,
                    });
                }
                TrpcRouterEntry::Nested {
                    key,
                    entries: inner,
                } => {
                    let mut new_path = path.to_vec();
                    new_path.push(key.clone());
                    compose_entries(
                        inner, origin_rel, &new_path, by_key, ref_target, ancestry, out,
                    );
                }
                TrpcRouterEntry::Ref {
                    key,
                    ident,
                    specifier,
                } => {
                    let Some(target_key) = ref_target(origin_rel, ident, specifier) else {
                        continue; // unresolvable specifier, or no fragment named `ident` there — skip, never guess
                    };
                    if ancestry.contains(&target_key) {
                        continue; // cycle guard
                    }
                    let target_frag = by_key[&target_key];
                    let new_path = if key.is_empty() {
                        path.to_vec() // mergeRouters splice-in-place — no path segment added
                    } else {
                        let mut p = path.to_vec();
                        p.push(key.clone());
                        p
                    };
                    ancestry.push(target_key.clone());
                    compose_entries(
                        &target_frag.entries,
                        &target_key.0,
                        &new_path,
                        by_key,
                        ref_target,
                        ancestry,
                        out,
                    );
                    ancestry.pop();
                }
            }
        }
    }

    let mut out = Vec::new();
    for root_key in roots {
        let frag = by_key[&root_key];
        let mut ancestry = vec![root_key.clone()];
        compose_entries(
            &frag.entries,
            &root_key.0,
            &[],
            &by_key,
            &ref_target,
            &mut ancestry,
            &mut out,
        );
    }

    let mut seen: HashSet<(String, String, String, u32)> = HashSet::new();
    out.retain(|p| seen.insert((p.kind.clone(), p.key.clone(), p.file.clone(), p.line)));
    out.sort_by(|a, b| {
        a.kind
            .cmp(&b.kind)
            .then_with(|| a.key.cmp(&b.key))
            .then_with(|| a.file.cmp(&b.file))
            .then_with(|| a.line.cmp(&b.line))
    });
    out
}

/// Join per-file wrapper CALL fragments against wrapper DEFINITION fragments and emit an `http`
/// `IoConsume` at each resolvable CALL site — the consume-side twin of the provide composers: the
/// wrapper's own body only ever shows egress a non-literal sink (`axios.request(options)`), so
/// without this join a project-local request-wrapper family is invisible and every consume anchor
/// points at wrapper internals instead of the code a reader would edit.
///
/// Resolution: a call's `callee` finds its def in the SAME file first (local wrapper), else via
/// `resolve(specifier, from_file)` → that file's def of the same name (the same workspace-aware
/// resolver the provide composers use). Method = the def's `fixed_method` or the call's
/// `method_param`-indexed arg (must be a literal GET/POST/PUT/PATCH/DELETE — anything else skips
/// the call, never guesses); path = the `path_param`-indexed arg (must start with `/`). Emitted
/// consumes are fully keyed (no late resolution) and deduped/sorted deterministically.
pub(crate) fn resolve_wrapper_consumes(
    def_pairs: Vec<(String, Vec<zzop_core::WrapperDefFragment>)>,
    call_pairs: Vec<(String, Vec<zzop_core::WrapperCallFragment>)>,
    resolve: impl Fn(&str, &str) -> Option<String>,
    io_consumes: &mut Vec<IoConsume>,
) {
    let mut defs: HashMap<(String, String), &zzop_core::WrapperDefFragment> = HashMap::new();
    for (file, frags) in &def_pairs {
        for def in frags {
            defs.insert((file.clone(), def.name.clone()), def);
        }
    }

    let mut call_pairs = call_pairs;
    call_pairs.sort_by(|a, b| a.0.cmp(&b.0));

    let mut out: Vec<IoConsume> = Vec::new();
    for (file, calls) in &call_pairs {
        for call in calls {
            let def_file = match &call.specifier {
                None => Some(file.clone()),
                Some(spec) => resolve(spec, file),
            };
            let def = def_file.and_then(|f| defs.get(&(f, call.callee.clone())).copied());
            let Some(def) = def else { continue };
            let method = match (&def.fixed_method, def.method_param) {
                (Some(m), _) => Some(m.clone()),
                // Any-case verb literal accepted and uppercased — the same tolerance
                // `egress::method_from_options` applies (its own tests use `method: "delete"`).
                (None, Some(idx)) => call
                    .args
                    .get(idx as usize)
                    .and_then(|a| a.clone())
                    .map(|m| m.to_ascii_uppercase())
                    // Verb vocabulary is the core T1 single source, not a local copy (policy census).
                    .filter(|m| zzop_core::HTTP_KEY_VERBS.contains(&m.as_str())),
                (None, None) => None,
            };
            let Some(method) = method else { continue };
            let path = call
                .args
                .get(def.path_param as usize)
                .and_then(|a| a.clone())
                .filter(|p| p.starts_with('/'));
            let Some(path) = path else { continue };
            out.push(IoConsume {
                client: None,
                body: None,
                kind: "http".to_string(),
                key: Some(zzop_core::http_consume_interface_key(&method, &path)),
                file: file.clone(),
                line: call.line,
                raw: None,
                method: None,
            });
        }
    }

    out.sort_by(|a, b| {
        a.key
            .cmp(&b.key)
            .then_with(|| a.file.cmp(&b.file))
            .then_with(|| a.line.cmp(&b.line))
    });
    out.dedup_by(|a, b| a.key == b.key && a.file == b.file && a.line == b.line);
    io_consumes.extend(out);
}

/// Axios `axios.defaults.baseURL` path-prefix apply + strip (`axios-defaults-base-v1`) — the
/// CONSUME-side counterpart of [`apply_and_strip_global_prefix`]: see
/// `zzop_parser_typescript::adapters::client_base`'s module doc for why this rides the `consumes`
/// channel as a `"client-base-prefix"` sentinel (mirrors the `"nest-global-prefix"` sentinel-string
/// convention that function's own doc describes) instead of a dedicated field.
///
/// ## Grouping
/// Sentinels are grouped by `client` (today only `"axios"` is ever emitted, but the grouping itself is
/// generic) so a future second recognizer's own base prefix can never cross-contaminate axios's. Within
/// one client group:
/// - Exactly one distinct sentinel path: every `http` consume tagged with that SAME `client` gets the
///   path prepended (see "Apply" below).
/// - 2+ distinct sentinel paths: nothing is applied for that client — ONE aggregated warning names the
///   client, every distinct path, and the declaring `file:line` of each sentinel (honest degrade over
///   guessing which one is real, same stance as [`apply_and_strip_global_prefix`]'s own multi-value
///   case).
/// - A sentinel with `key: None`, or a path that normalizes to empty/`"/"`: skipped defensively —
///   `extract_client_base_prefix_marker` never emits these per its own doc, but this seam does not rely
///   on that invariant holding.
///
/// ## Apply
/// A consume is rewritten only when ALL of: `kind == "http"`, `client` equals the resolved client, `key`
/// is `Some` (an unresolved consume is left exactly as unresolved — never guessed), and the key's path
/// (everything after the first space) starts with `/` and does not carry a scheme (`://` — an absolute
/// URL axios's `baseURL` never applies to). A matching key `"METHOD /path"` becomes
/// `"METHOD /<prefix>/path"` — deliberately prepended even when `/path` already starts with the prefix
/// (`"/api"` + `"/api/users"` -> `"/api/api/users"`), mirroring what the axios runtime actually does.
///
/// In every case every `"client-base-prefix"` sentinel is stripped from `io_consumes` unconditionally
/// (even when conflicting/unapplied) — it must never reach output, the linker, or rules.
///
/// ## Placement (load-bearing)
/// Must run AFTER [`late_resolve_cross_file_consumes`] — that pass fills `key` IN PLACE and preserves
/// the `client` tag, so a late-resolved axios consume still gets the prefix; this tag preservation is
/// the load-bearing ordering constraint. Sitting after [`resolve_wrapper_consumes`] is only "after the
/// last consume-mutating pass" hygiene: wrapper-emitted consumes carry `client: None` and are
/// DELIBERATELY never prefixed (custom wrappers stay uninterpreted — overlay territory). Must stay
/// BEFORE `io_consumes` is frozen into `MinimalIr::io` / read by any whole-tree rule
/// (`unprovided-consume`) or the cross-layer linker — see `zzop_engine::analyze::mod`'s call site.
pub(crate) fn apply_client_base_prefixes(
    io_consumes: &mut Vec<IoConsume>,
    warnings: &mut Vec<String>,
) {
    // Bound to the parser's exported const (not a local literal) so a rename on the emit side
    // cannot silently desynchronize the strip side — a leaked sentinel would reach output.
    const SENTINEL_KIND: &str = zzop_parser_typescript::CLIENT_BASE_PREFIX_KIND;

    // client -> every sentinel naming it: (normalized path, file, line) — collected before any mutation
    // so the resolve step below can see every candidate for a client regardless of iteration order.
    let mut by_client: BTreeMap<String, Vec<(String, String, u32)>> = BTreeMap::new();
    for c in io_consumes.iter() {
        if c.kind != SENTINEL_KIND {
            continue;
        }
        let Some(client) = c.client.clone() else {
            continue; // defensive: the parser always tags a sentinel's client
        };
        let Some(key) = c.key.as_deref() else {
            continue; // defensive: the parser never emits a keyless sentinel
        };
        let path = key.trim_matches('/');
        if path.is_empty() {
            continue; // defensive: an empty/"/" base has no path part to prepend
        }
        by_client
            .entry(client)
            .or_default()
            .push((path.to_string(), c.file.clone(), c.line));
    }

    // Resolve each client group to exactly one applicable prefix, or none (never-guess on conflict).
    let mut prefixes: HashMap<String, String> = HashMap::new();
    for (client, entries) in &by_client {
        let mut distinct: Vec<&str> = entries.iter().map(|(p, _, _)| p.as_str()).collect();
        distinct.sort();
        distinct.dedup();
        match distinct.as_slice() {
            [] => {}
            [path] => {
                prefixes.insert(client.clone(), (*path).to_string());
            }
            _ => {
                let mut sorted_entries = entries.clone();
                sorted_entries.sort_by(|a, b| {
                    a.0.cmp(&b.0)
                        .then_with(|| a.1.cmp(&b.1))
                        .then_with(|| a.2.cmp(&b.2))
                });
                let detail: Vec<String> = sorted_entries
                    .iter()
                    .map(|(p, f, l)| format!("/{p} ({f}:{l})"))
                    .collect();
                warnings.push(format!(
                    "multiple axios.defaults.baseURL values found for client `{client}`: [{}]; skipping baseURL prefix rewrite",
                    detail.join(", ")
                ));
            }
        }
    }

    for c in io_consumes.iter_mut() {
        if c.kind != "http" {
            continue;
        }
        let Some(client) = c.client.as_deref() else {
            continue;
        };
        let Some(prefix) = prefixes.get(client) else {
            continue;
        };
        let Some(key) = c.key.as_deref() else {
            continue; // unresolved — never guessed
        };
        let Some((verb, path)) = key.split_once(' ') else {
            continue; // defensive: never produced by http_consume_interface_key
        };
        if !path.starts_with('/') || path.contains("://") {
            continue; // external/absolute-URL key — axios ignores baseURL for those
        }
        c.key = Some(format!("{verb} /{prefix}{path}"));
    }

    io_consumes.retain(|c| c.kind != SENTINEL_KIND);
}

/// Compose whole-tree `http` PROVIDEs from per-file router-mount fragments
/// (`zzop_parser_typescript::router_mounts` — Hono-style chained builders and cross-file
/// `.route(prefix, subRouter)` mounts). The provide-side twin of [`compose_trpc_provides`]: same
/// root-exclusion conservatism, same import-resolver closure, same dedup/sort discipline; composition
/// joins URL path prefixes instead of dotted procedure paths.
///
/// **Roots**: a fragment is a DFS root only when nothing mounts it — neither by resolved edge nor by
/// NAME (some `Mount.ident` anywhere equals its name, even unresolved): a mounted-but-unresolvable
/// sub-router must not surface its entries with a truncated (missing prefix) URL — under-reporting is
/// honest, mis-keying is not.
///
/// **Mount resolution**: same-file fragment named `ident` first; else `resolve(specifier)` →
/// target file, preferring the fragment named `ident` and falling back to the file's SOLE fragment
/// (covers `export default route` re-imported under an arbitrary local alias — the common
/// one-router-per-file layout). Ambiguous (multi-fragment, no name match) or unresolvable mounts
/// skip that subtree.
///
/// Provide anchors: `file`/`line` of the VERB registration (the leaf file, not the mount site),
/// `symbol` = handler name — the place a reader would edit the route.
pub(crate) fn compose_router_mount_provides(
    fragments: Vec<(String, Vec<zzop_core::RouterMountFragment>)>,
    resolve: impl Fn(&str, &str) -> Option<String>,
) -> Vec<IoProvide> {
    use zzop_core::{RouterMountEntry, RouterMountFragment};

    let mut fragments = fragments;
    fragments.sort_by(|a, b| a.0.cmp(&b.0));

    // (file, fragment) node list + per-file index; nodes keep per-file source order.
    let mut nodes: Vec<(&str, &RouterMountFragment)> = Vec::new();
    let mut by_file: HashMap<&str, Vec<usize>> = HashMap::new();
    for (file, frags) in &fragments {
        for frag in frags {
            by_file.entry(file.as_str()).or_default().push(nodes.len());
            nodes.push((file.as_str(), frag));
        }
    }

    let find_child = |from_file: &str, ident: &str, specifier: Option<&str>| -> Option<usize> {
        let candidates_in = |file: &str| -> Option<usize> {
            let idxs = by_file.get(file)?;
            if let Some(&idx) = idxs.iter().find(|&&i| nodes[i].1.name == ident) {
                return Some(idx);
            }
            if idxs.len() == 1 {
                return Some(idxs[0]);
            }
            None
        };
        match specifier {
            None => candidates_in(from_file),
            Some(spec) => {
                let target = resolve(spec, from_file)?;
                candidates_in(&target)
            }
        }
    };

    // Root exclusion: mounted by name anywhere (unresolved-conservative) OR by resolved edge.
    let mut mounted_names: HashSet<&str> = HashSet::new();
    let mut mounted_nodes: HashSet<usize> = HashSet::new();
    for (file, frag) in &nodes {
        for entry in &frag.entries {
            if let RouterMountEntry::Mount {
                ident, specifier, ..
            } = entry
            {
                mounted_names.insert(ident.as_str());
                if let Some(child) = find_child(file, ident, specifier.as_deref()) {
                    mounted_nodes.insert(child);
                }
            }
        }
    }

    fn join_prefix(prefix: &str, seg: &str) -> String {
        if seg == "/" || seg.is_empty() {
            return prefix.to_string();
        }
        let base = prefix.trim_end_matches('/');
        if seg.starts_with('/') {
            format!("{base}{seg}")
        } else {
            format!("{base}/{seg}")
        }
    }

    /// `(from_file, ident, specifier)` → node index of the mounted child fragment, if resolvable.
    type FindChild<'a> = dyn Fn(&str, &str, Option<&str>) -> Option<usize> + 'a;

    #[allow(clippy::too_many_arguments)]
    fn walk(
        idx: usize,
        prefix: &str,
        nodes: &[(&str, &zzop_core::RouterMountFragment)],
        find_child: &FindChild,
        ancestry: &mut Vec<usize>,
        out: &mut Vec<IoProvide>,
    ) {
        if ancestry.contains(&idx) {
            return; // cycle guard — mirrors compose_trpc_provides' ancestry stack
        }
        ancestry.push(idx);
        let (file, frag) = nodes[idx];
        for entry in &frag.entries {
            match entry {
                zzop_core::RouterMountEntry::Verb {
                    method,
                    path,
                    handler,
                    line,
                } => {
                    let full = join_prefix(prefix, path);
                    out.push(IoProvide {
                        body: None,
                        kind: "http".to_string(),
                        key: zzop_core::http_interface_key(method, &full),
                        file: file.to_string(),
                        line: *line,
                        symbol: handler.clone(),
                    });
                }
                zzop_core::RouterMountEntry::Mount {
                    prefix: mount_prefix,
                    ident,
                    specifier,
                } => {
                    if let Some(child) = find_child(file, ident, specifier.as_deref()) {
                        walk(
                            child,
                            &join_prefix(prefix, mount_prefix),
                            nodes,
                            find_child,
                            ancestry,
                            out,
                        );
                    }
                }
            }
        }
        ancestry.pop();
    }

    let mut out: Vec<IoProvide> = Vec::new();
    for idx in 0..nodes.len() {
        if mounted_nodes.contains(&idx) || mounted_names.contains(nodes[idx].1.name.as_str()) {
            continue;
        }
        let mut ancestry = Vec::new();
        walk(idx, "", &nodes, &find_child, &mut ancestry, &mut out);
    }

    out.sort_by(|a, b| {
        a.key
            .cmp(&b.key)
            .then_with(|| a.file.cmp(&b.file))
            .then_with(|| a.line.cmp(&b.line))
    });
    out.dedup_by(|a, b| a.key == b.key && a.file == b.file && a.line == b.line);
    out
}

#[cfg(test)]
mod late_resolve_tests {
    use super::*;

    fn unresolved(raw: &str, method: &str) -> IoConsume {
        IoConsume {
            client: None,
            body: None,
            kind: "http".to_string(),
            key: None,
            file: "src/caller.ts".to_string(),
            line: 1,
            raw: Some(raw.to_string()),
            method: Some(method.to_string()),
        }
    }

    fn consts(entries: &[(&str, &str)]) -> Vec<(String, HashMap<String, String>)> {
        let fragment: HashMap<String, String> = entries
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        vec![("src/consts.ts".to_string(), fragment)]
    }

    #[test]
    fn slash_value_resolves_to_a_normalized_internal_key() {
        let mut consumes = vec![unresolved("Api.user", "GET")];
        late_resolve_cross_file_consumes(consts(&[("Api.user", "/api/user/")]), &mut consumes);
        assert_eq!(consumes[0].key.as_deref(), Some("GET /api/user"));
        assert!(consumes[0].raw.is_some()); // provenance retained
    }

    #[test]
    fn absolute_url_value_keeps_the_verbatim_external_key() {
        let mut consumes = vec![unresolved("Api.vendor", "POST")];
        late_resolve_cross_file_consumes(
            consts(&[("Api.vendor", "https://vendor.com/x")]),
            &mut consumes,
        );
        // Verbatim -- `link_cross_layer_io`'s `"://"` gate must still see the host.
        assert_eq!(
            consumes[0].key.as_deref(),
            Some("POST https://vendor.com/x")
        );
    }

    #[test]
    fn base_relative_fragment_value_keys_root_normalized() {
        // Intent change (`base-relative-egress-v1`, cross-layer-resolution decision 2026-07-10): a
        // path-shaped fragment (`authen/getUserInfo` — the axios `baseURL` idiom) keys as its
        // root-normalized path instead of staying unresolved, mirroring the egress extractor's gating.
        let mut consumes = vec![unresolved("Api.frag", "GET")];
        late_resolve_cross_file_consumes(
            consts(&[("Api.frag", "authen/getUserInfo")]),
            &mut consumes,
        );
        assert_eq!(consumes[0].key.as_deref(), Some("GET /authen/getUserInfo"));
    }

    #[test]
    fn non_path_shaped_fragment_value_stays_unresolved() {
        // The never-guess veto list survives the intent change: a document-relative `./` value and a
        // whitespace-carrying value are not base-relative paths.
        let mut consumes = vec![unresolved("Api.rel", "GET"), unresolved("Api.txt", "GET")];
        late_resolve_cross_file_consumes(
            consts(&[("Api.rel", "./authen"), ("Api.txt", "not a path")]),
            &mut consumes,
        );
        assert_eq!(consumes[0].key, None);
        assert_eq!(consumes[1].key, None);
    }
}

#[cfg(test)]
mod global_prefix_tests {
    //! Coverage for `apply_and_strip_global_prefix`: the single-prefix rewrite (with and without a
    //! leading slash in source), the no-marker no-op, and the multiple-distinct-values honest degrade.
    use super::*;

    fn http_provide(key: &str, file: &str) -> IoProvide {
        IoProvide {
            body: None,
            kind: "http".to_string(),
            key: key.to_string(),
            file: file.to_string(),
            line: 1,
            symbol: None,
        }
    }

    fn prefix_marker(key: &str) -> IoProvide {
        IoProvide {
            body: None,
            kind: zzop_parser_typescript::NEST_GLOBAL_PREFIX_KIND.to_string(),
            key: key.to_string(),
            file: "main.ts".to_string(),
            line: 1,
            symbol: None,
        }
    }

    #[test]
    fn single_prefix_rewrites_http_provides_and_strips_the_sentinel() {
        let mut provides = vec![
            http_provide("GET /articles", "articles.controller.ts"),
            prefix_marker("api"),
        ];
        let mut warnings = Vec::new();
        apply_and_strip_global_prefix(&mut provides, &mut warnings);
        assert_eq!(provides.len(), 1);
        assert_eq!(provides[0].key, "GET /api/articles");
        assert!(warnings.is_empty());
        assert!(provides.iter().all(|p| p.kind != "nest-global-prefix"));
    }

    #[test]
    fn no_marker_leaves_http_provides_unchanged() {
        let mut provides = vec![http_provide("GET /articles", "articles.controller.ts")];
        let mut warnings = Vec::new();
        apply_and_strip_global_prefix(&mut provides, &mut warnings);
        assert_eq!(provides.len(), 1);
        assert_eq!(provides[0].key, "GET /articles");
        assert!(warnings.is_empty());
    }

    #[test]
    fn leading_slash_in_source_still_yields_exactly_one_slash_at_the_seam() {
        let mut provides = vec![
            http_provide("GET /articles", "articles.controller.ts"),
            prefix_marker("/api"),
        ];
        let mut warnings = Vec::new();
        apply_and_strip_global_prefix(&mut provides, &mut warnings);
        assert_eq!(provides[0].key, "GET /api/articles");
    }

    #[test]
    fn root_path_prefix_collapses_onto_the_prefix_alone() {
        let mut provides = vec![
            http_provide("GET /", "app.controller.ts"),
            prefix_marker("api"),
        ];
        let mut warnings = Vec::new();
        apply_and_strip_global_prefix(&mut provides, &mut warnings);
        assert_eq!(provides[0].key, "GET /api");
    }

    #[test]
    fn multiple_distinct_prefixes_skip_the_rewrite_and_warn() {
        let mut provides = vec![
            http_provide("GET /articles", "articles.controller.ts"),
            prefix_marker("api"),
            prefix_marker("v2"),
        ];
        let mut warnings = Vec::new();
        apply_and_strip_global_prefix(&mut provides, &mut warnings);
        assert_eq!(provides.len(), 1);
        assert_eq!(provides[0].key, "GET /articles"); // unchanged — never guess
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("multiple setGlobalPrefix values found"));
        assert!(warnings[0].contains("api"));
        assert!(warnings[0].contains("v2"));
    }

    #[test]
    fn only_http_kind_provides_are_rewritten() {
        // A non-"http" provide (e.g. "trpc") must not be touched by the rewrite.
        let mut provides = vec![
            IoProvide {
                body: None,
                kind: "trpc".to_string(),
                key: "GET /articles".to_string(),
                file: "t.ts".to_string(),
                line: 1,
                symbol: None,
            },
            prefix_marker("api"),
        ];
        let mut warnings = Vec::new();
        apply_and_strip_global_prefix(&mut provides, &mut warnings);
        assert_eq!(provides.len(), 1);
        assert_eq!(provides[0].key, "GET /articles");
    }

    #[test]
    fn trailing_slash_prefix_yields_exactly_one_slash_at_the_seam() {
        let mut provides = vec![
            http_provide("GET /articles", "articles.controller.ts"),
            prefix_marker("api/"),
        ];
        let mut warnings = Vec::new();
        apply_and_strip_global_prefix(&mut provides, &mut warnings);
        assert_eq!(provides[0].key, "GET /api/articles");
    }

    #[test]
    fn empty_string_prefix_is_a_no_op_but_still_strips_the_sentinel() {
        let mut provides = vec![
            http_provide("GET /articles", "articles.controller.ts"),
            prefix_marker(""),
        ];
        let mut warnings = Vec::new();
        apply_and_strip_global_prefix(&mut provides, &mut warnings);
        assert_eq!(provides.len(), 1);
        assert_eq!(provides[0].key, "GET /articles"); // unchanged — empty prefix means no prefix
        assert!(warnings.is_empty());
        assert!(provides.iter().all(|p| p.kind != "nest-global-prefix"));
    }

    #[test]
    fn bare_slash_prefix_is_a_no_op_but_still_strips_the_sentinel() {
        let mut provides = vec![
            http_provide("GET /articles", "articles.controller.ts"),
            prefix_marker("/"),
        ];
        let mut warnings = Vec::new();
        apply_and_strip_global_prefix(&mut provides, &mut warnings);
        assert_eq!(provides.len(), 1);
        assert_eq!(provides[0].key, "GET /articles");
        assert!(provides.iter().all(|p| p.kind != "nest-global-prefix"));
    }

    #[test]
    fn multi_segment_prefix_is_prepended_whole() {
        let mut provides = vec![
            http_provide("GET /articles", "articles.controller.ts"),
            prefix_marker("api/v1"),
        ];
        let mut warnings = Vec::new();
        apply_and_strip_global_prefix(&mut provides, &mut warnings);
        assert_eq!(provides[0].key, "GET /api/v1/articles");
    }

    #[test]
    fn param_placeholder_in_the_path_is_preserved_across_the_rewrite() {
        let mut provides = vec![
            http_provide("DELETE /articles/{}", "articles.controller.ts"),
            prefix_marker("api"),
        ];
        let mut warnings = Vec::new();
        apply_and_strip_global_prefix(&mut provides, &mut warnings);
        assert_eq!(provides[0].key, "DELETE /api/articles/{}");
    }

    #[test]
    fn only_provides_present_at_the_seam_get_prefixed() {
        // Scope guard for the moved seam: whatever `http` provides are present when this runs get
        // prefixed; a provide APPENDED afterwards (e.g. a Java/Hono/file-route provide, which in the
        // real pipeline is added only after this call) is untouched because it isn't in the vec yet.
        let mut provides = vec![
            http_provide("GET /articles", "articles.controller.ts"),
            prefix_marker("api"),
        ];
        let mut warnings = Vec::new();
        apply_and_strip_global_prefix(&mut provides, &mut warnings);
        // Simulate a later producer appending an already-complete route path.
        provides.push(http_provide("GET /api/foo", "pages/api/foo.ts"));
        assert_eq!(provides.len(), 2);
        assert_eq!(provides[0].key, "GET /api/articles"); // was present -> prefixed
        assert_eq!(provides[1].key, "GET /api/foo"); // appended after -> NOT double-prefixed
    }

    // --- prepend_global_prefix unit coverage (the seam join, prefix already normalized) ---

    #[test]
    fn prepend_produces_one_clean_slash_and_preserves_verb_and_params() {
        assert_eq!(
            prepend_global_prefix("GET /articles", "api"),
            "GET /api/articles"
        );
        assert_eq!(prepend_global_prefix("GET /", "api"), "GET /api");
        assert_eq!(
            prepend_global_prefix("DELETE /articles/{}", "api/v1"),
            "DELETE /api/v1/articles/{}"
        );
    }
}

#[cfg(test)]
mod wrapper_consume_tests {
    //! Coverage for `resolve_wrapper_consumes`: cross-file join via specifier, same-file local
    //! wrapper, fixed-method wrappers, the never-guess skips (non-verb method arg, non-`/` path,
    //! unresolvable specifier), and determinism.
    use super::*;
    use zzop_core::{WrapperCallFragment, WrapperDefFragment};

    fn def(
        name: &str,
        method_param: Option<u32>,
        path_param: u32,
        fixed: Option<&str>,
    ) -> WrapperDefFragment {
        WrapperDefFragment {
            name: name.to_string(),
            method_param,
            path_param,
            fixed_method: fixed.map(str::to_string),
        }
    }

    fn call(
        callee: &str,
        specifier: Option<&str>,
        args: Vec<Option<&str>>,
        line: u32,
    ) -> WrapperCallFragment {
        WrapperCallFragment {
            callee: callee.to_string(),
            specifier: specifier.map(str::to_string),
            args: args.into_iter().map(|a| a.map(str::to_string)).collect(),
            line,
        }
    }

    fn resolver<'a>(
        map: &'a [(&'a str, &'a str, &'a str)],
    ) -> impl Fn(&str, &str) -> Option<String> + 'a {
        move |spec: &str, from: &str| {
            map.iter()
                .find(|(s, f, _)| *s == spec && *f == from)
                .map(|(_, _, t)| t.to_string())
        }
    }

    #[test]
    fn imported_wrapper_call_becomes_a_keyed_consume_at_the_call_site() {
        let defs = vec![(
            "utils/api.ts".to_string(),
            vec![def("makeRestApiRequest", Some(1), 2, None)],
        )];
        let calls = vec![(
            "src/api/workflows.ts".to_string(),
            vec![
                call(
                    "makeRestApiRequest",
                    Some("@/utils/api"),
                    vec![None, Some("GET"), Some("/workflows/new")],
                    12,
                ),
                call(
                    "makeRestApiRequest",
                    Some("@/utils/api"),
                    vec![None, Some("POST"), Some("/workflows/{}/activate"), None],
                    30,
                ),
            ],
        )];
        let mut consumes = Vec::new();
        resolve_wrapper_consumes(
            defs,
            calls,
            resolver(&[("@/utils/api", "src/api/workflows.ts", "utils/api.ts")]),
            &mut consumes,
        );
        let keys: Vec<&str> = consumes.iter().flat_map(|c| c.key.as_deref()).collect();
        assert_eq!(
            keys,
            vec!["GET /workflows/new", "POST /workflows/{}/activate"]
        );
        assert_eq!(consumes[0].file, "src/api/workflows.ts");
        assert_eq!(consumes[0].line, 12);
    }

    #[test]
    fn fixed_method_wrapper_and_same_file_local_call() {
        let defs = vec![(
            "src/stream.ts".to_string(),
            vec![def("streamRequest", None, 1, Some("POST"))],
        )];
        let calls = vec![(
            "src/stream.ts".to_string(),
            vec![call(
                "streamRequest",
                None,
                vec![None, Some("/ai/chat")],
                40,
            )],
        )];
        let mut consumes = Vec::new();
        resolve_wrapper_consumes(defs, calls, |_, _| None, &mut consumes);
        assert_eq!(consumes.len(), 1);
        assert_eq!(consumes[0].key.as_deref(), Some("POST /ai/chat"));
    }

    #[test]
    fn never_guesses_on_non_verb_non_path_or_unresolvable() {
        let defs = vec![(
            "utils/api.ts".to_string(),
            vec![def("makeRestApiRequest", Some(1), 2, None)],
        )];
        let calls = vec![(
            "src/a.ts".to_string(),
            vec![
                // method arg is a variable, not a literal verb
                call(
                    "makeRestApiRequest",
                    Some("./u"),
                    vec![None, None, Some("/x")],
                    1,
                ),
                // path arg does not start with '/'
                call(
                    "makeRestApiRequest",
                    Some("./u"),
                    vec![None, Some("GET"), Some("x")],
                    2,
                ),
                // unresolvable specifier
                call(
                    "makeRestApiRequest",
                    Some("./nowhere"),
                    vec![None, Some("GET"), Some("/x")],
                    3,
                ),
            ],
        )];
        let mut consumes = Vec::new();
        resolve_wrapper_consumes(
            defs,
            calls,
            resolver(&[("./u", "src/a.ts", "utils/api.ts")]),
            &mut consumes,
        );
        assert!(consumes.is_empty());
    }

    #[test]
    fn output_is_deterministic_across_input_order() {
        let defs = vec![("u.ts".to_string(), vec![def("w", Some(0), 1, None)])];
        let build = |rev: bool| {
            let mut v = vec![
                (
                    "a.ts".to_string(),
                    vec![call("w", Some("./u"), vec![Some("GET"), Some("/a")], 1)],
                ),
                (
                    "b.ts".to_string(),
                    vec![call("w", Some("./u"), vec![Some("GET"), Some("/b")], 1)],
                ),
            ];
            if rev {
                v.reverse();
            }
            v
        };
        let run = |calls| {
            let mut out = Vec::new();
            resolve_wrapper_consumes(
                defs.clone(),
                calls,
                resolver(&[("./u", "a.ts", "u.ts"), ("./u", "b.ts", "u.ts")]),
                &mut out,
            );
            out.into_iter()
                .map(|c| (c.key, c.file, c.line))
                .collect::<Vec<_>>()
        };
        assert_eq!(run(build(false)), run(build(true)));
    }
}

#[cfg(test)]
mod router_mount_compose_tests {
    //! Coverage for `compose_router_mount_provides`: same-file mount join, a 3-hop mount chain across
    //! files, `/`-prefix passthrough, root exclusion (mounted child never emitted unprefixed;
    //! unresolvable-but-named child skipped wholesale), sole-fragment fallback for default-import
    //! aliases, cycle guard, determinism.
    use super::*;
    use zzop_core::{RouterMountEntry, RouterMountFragment};

    fn verb(method: &str, path: &str, handler: &str, line: u32) -> RouterMountEntry {
        RouterMountEntry::Verb {
            method: method.to_string(),
            path: path.to_string(),
            handler: Some(handler.to_string()),
            line,
        }
    }

    fn mount(prefix: &str, ident: &str, specifier: Option<&str>) -> RouterMountEntry {
        RouterMountEntry::Mount {
            prefix: prefix.to_string(),
            ident: ident.to_string(),
            specifier: specifier.map(str::to_string),
        }
    }

    fn frag(name: &str, entries: Vec<RouterMountEntry>) -> RouterMountFragment {
        RouterMountFragment {
            name: name.to_string(),
            entries,
        }
    }

    fn no_resolver() -> impl Fn(&str, &str) -> Option<String> {
        |_: &str, _: &str| None
    }

    /// Maps (specifier, from_file) pairs to target rel paths.
    fn resolver<'a>(
        map: &'a [(&'a str, &'a str, &'a str)],
    ) -> impl Fn(&str, &str) -> Option<String> + 'a {
        move |spec: &str, from: &str| {
            map.iter()
                .find(|(s, f, _)| *s == spec && *f == from)
                .map(|(_, _, t)| t.to_string())
        }
    }

    #[test]
    fn same_file_mount_joins_prefix() {
        let out = compose_router_mount_provides(
            vec![(
                "src/app.ts".to_string(),
                vec![
                    frag(
                        "app",
                        vec![
                            verb("GET", "/health", "h", 2),
                            mount("/admin", "adminRouter", None),
                        ],
                    ),
                    frag("adminRouter", vec![verb("POST", "/users", "createUser", 9)]),
                ],
            )],
            no_resolver(),
        );
        let keys: Vec<&str> = out.iter().map(|p| p.key.as_str()).collect();
        assert_eq!(keys, vec!["GET /health", "POST /admin/users"]);
        assert_eq!(out[1].file, "src/app.ts");
        assert_eq!(out[1].line, 9);
        assert_eq!(out[1].symbol.as_deref(), Some("createUser"));
    }

    #[test]
    fn three_hop_mount_chain_composes_full_url() {
        // server/router.ts mounts auth at /api/auth; auth/index.ts mounts twoFactorRoute at
        // /two-factor (plus an inline verb and a "/"-passthrough mount); the leaf file registers
        // POST /setup. Expected: /api/auth/two-factor/setup with the LEAF file:line anchor.
        let fragments = vec![
            (
                "auth/index.ts".to_string(),
                vec![frag(
                    "auth",
                    vec![
                        verb("GET", "/csrf", "csrfHandler", 21),
                        mount("/", "sessionRoute", Some("./routes/session")),
                        mount("/two-factor", "twoFactorRoute", Some("./routes/two-factor")),
                    ],
                )],
            ),
            (
                "auth/routes/session.ts".to_string(),
                vec![frag("sessionRoute", vec![verb("GET", "/session", "s", 5)])],
            ),
            (
                "auth/routes/two-factor.ts".to_string(),
                vec![frag(
                    "twoFactorRoute",
                    vec![verb("POST", "/setup", "setup", 20)],
                )],
            ),
            (
                "server/router.ts".to_string(),
                vec![frag(
                    "app",
                    vec![mount("/api/auth", "auth", Some("@example/auth-server"))],
                )],
            ),
        ];
        let out = compose_router_mount_provides(
            fragments,
            resolver(&[
                (
                    "./routes/session",
                    "auth/index.ts",
                    "auth/routes/session.ts",
                ),
                (
                    "./routes/two-factor",
                    "auth/index.ts",
                    "auth/routes/two-factor.ts",
                ),
                ("@example/auth-server", "server/router.ts", "auth/index.ts"),
            ]),
        );
        let keys: Vec<&str> = out.iter().map(|p| p.key.as_str()).collect();
        assert_eq!(
            keys,
            vec![
                "GET /api/auth/csrf",
                "GET /api/auth/session",
                "POST /api/auth/two-factor/setup",
            ]
        );
        assert_eq!(out[2].file, "auth/routes/two-factor.ts");
        assert_eq!(out[2].line, 20);
    }

    #[test]
    fn mounted_child_is_never_emitted_unprefixed_even_when_unresolvable() {
        // `admin` is mounted by name from a file the resolver cannot link — the child fragment
        // must NOT surface `/users` with the missing `/admin` prefix (conservative root
        // exclusion, mirroring compose_trpc_provides).
        let fragments = vec![
            (
                "src/app.ts".to_string(),
                vec![frag(
                    "app",
                    vec![mount("/admin", "admin", Some("./nowhere"))],
                )],
            ),
            (
                "src/admin.ts".to_string(),
                vec![frag("admin", vec![verb("GET", "/users", "h", 3)])],
            ),
        ];
        let out = compose_router_mount_provides(fragments, no_resolver());
        assert!(out.is_empty());
    }

    #[test]
    fn sole_fragment_fallback_covers_default_import_alias() {
        // `export default route` re-imported as `pdfRoute` — no name match in the target file,
        // but it holds exactly one fragment, so the mount resolves to it.
        let fragments = vec![
            (
                "server/files.ts".to_string(),
                vec![frag(
                    "filesRoute",
                    vec![mount("/", "pdfRoute", Some("./routes/pdf"))],
                )],
            ),
            (
                "server/routes/pdf.ts".to_string(),
                vec![frag(
                    "route",
                    vec![verb("GET", "/envelope/:id/item.pdf", "h", 4)],
                )],
            ),
        ];
        let out = compose_router_mount_provides(
            fragments,
            resolver(&[("./routes/pdf", "server/files.ts", "server/routes/pdf.ts")]),
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key, "GET /envelope/{}/item.pdf");
    }

    #[test]
    fn mount_cycle_is_guarded() {
        let fragments = vec![(
            "src/a.ts".to_string(),
            vec![
                frag("a", vec![verb("GET", "/x", "h", 1), mount("/b", "b", None)]),
                frag("b", vec![mount("/a", "a", None)]),
            ],
        )];
        let out = compose_router_mount_provides(fragments, no_resolver());
        // `a` and `b` mount each other, so neither is a root — conservative empty output rather
        // than an infinite walk or a truncated-prefix guess.
        assert!(out.is_empty());
    }

    #[test]
    fn output_is_deterministic_across_input_order() {
        let build = |rev: bool| {
            let mut v = vec![
                (
                    "src/app.ts".to_string(),
                    vec![frag(
                        "app",
                        vec![mount("/api", "sub", None), verb("GET", "/", "root", 1)],
                    )],
                ),
                (
                    "src/app.ts".to_string(),
                    vec![frag("sub", vec![verb("POST", "/items", "create", 8)])],
                ),
            ];
            if rev {
                v.reverse();
            }
            v
        };
        let a = compose_router_mount_provides(build(false), no_resolver());
        let b = compose_router_mount_provides(build(true), no_resolver());
        let view = |v: &[IoProvide]| -> Vec<(String, String, u32)> {
            v.iter()
                .map(|p| (p.key.clone(), p.file.clone(), p.line))
                .collect()
        };
        assert_eq!(view(&a), view(&b));
    }
}

#[cfg(test)]
mod trpc_compose_tests {
    //! Coverage for `compose_trpc_provides`: inline nested + leaf composition, cross-file `Ref` via
    //! specifier, same-file `Ref` by name, `mergeRouters` empty-key splice, an unresolvable `Ref` skipped
    //! (sibling entries survive), a self-referencing cycle guarded against infinite recursion, and
    //! determinism under input-order reshuffling.
    use super::*;
    use zzop_core::{TrpcRouterEntry, TrpcRouterFragment};

    /// A resolver that only ever answers the exact `(specifier, from_file)` pairs listed — anything else is
    /// `None`, mirroring how a real unresolvable/external specifier behaves.
    fn resolver(
        table: &'static [(&'static str, &'static str, &'static str)],
    ) -> impl Fn(&str, &str) -> Option<String> {
        move |specifier, from_file| {
            table
                .iter()
                .find(|(s, f, _)| *s == specifier && *f == from_file)
                .map(|(_, _, target)| target.to_string())
        }
    }

    fn no_resolver() -> impl Fn(&str, &str) -> Option<String> {
        |_, _| None
    }

    fn frag(name: &str, entries: Vec<TrpcRouterEntry>) -> TrpcRouterFragment {
        TrpcRouterFragment {
            name: name.to_string(),
            entries,
        }
    }

    fn keys(out: &[IoProvide]) -> Vec<(String, String, u32)> {
        out.iter()
            .map(|p| (p.key.clone(), p.file.clone(), p.line))
            .collect()
    }

    #[test]
    fn root_with_inline_nested_and_leaf() {
        let fragments = vec![(
            "a.ts".to_string(),
            vec![frag(
                "appRouter",
                vec![
                    TrpcRouterEntry::Nested {
                        key: "greeting".into(),
                        entries: vec![TrpcRouterEntry::Leaf {
                            key: "hello".into(),
                            verb: "QUERY".into(),
                            line: 2,
                        }],
                    },
                    TrpcRouterEntry::Leaf {
                        key: "ping".into(),
                        verb: "QUERY".into(),
                        line: 5,
                    },
                ],
            )],
        )];
        let out = compose_trpc_provides(fragments, no_resolver());
        assert_eq!(
            keys(&out),
            vec![
                ("QUERY greeting.hello".to_string(), "a.ts".to_string(), 2),
                ("QUERY ping".to_string(), "a.ts".to_string(), 5),
            ]
        );
    }

    #[test]
    fn ref_via_specifier_resolves_to_another_files_fragment() {
        let fragments = vec![
            (
                "trpc.ts".to_string(),
                vec![frag(
                    "appRouter",
                    vec![TrpcRouterEntry::Ref {
                        key: "viewer".into(),
                        ident: "viewerRouter".into(),
                        specifier: Some("./viewer".into()),
                    }],
                )],
            ),
            (
                "viewer.ts".to_string(),
                vec![frag(
                    "viewerRouter",
                    vec![TrpcRouterEntry::Leaf {
                        key: "me".into(),
                        verb: "QUERY".into(),
                        line: 1,
                    }],
                )],
            ),
        ];
        let out =
            compose_trpc_provides(fragments, resolver(&[("./viewer", "trpc.ts", "viewer.ts")]));
        assert_eq!(
            keys(&out),
            vec![("QUERY viewer.me".to_string(), "viewer.ts".to_string(), 1)]
        );
    }

    #[test]
    fn same_file_ref_by_name_has_no_specifier() {
        let fragments = vec![(
            "r.ts".to_string(),
            vec![
                frag(
                    "outer",
                    vec![TrpcRouterEntry::Ref {
                        key: "nested".into(),
                        ident: "inner".into(),
                        specifier: None,
                    }],
                ),
                frag(
                    "inner",
                    vec![TrpcRouterEntry::Leaf {
                        key: "x".into(),
                        verb: "QUERY".into(),
                        line: 3,
                    }],
                ),
            ],
        )];
        let out = compose_trpc_provides(fragments, no_resolver());
        assert_eq!(
            keys(&out),
            vec![("QUERY nested.x".to_string(), "r.ts".to_string(), 3)]
        );
    }

    #[test]
    fn merge_routers_empty_key_splices_at_the_current_level() {
        let fragments = vec![
            (
                "r.ts".to_string(),
                vec![frag(
                    "combined",
                    vec![
                        TrpcRouterEntry::Ref {
                            key: String::new(),
                            ident: "aRouter".into(),
                            specifier: Some("./a".into()),
                        },
                        TrpcRouterEntry::Ref {
                            key: String::new(),
                            ident: "bRouter".into(),
                            specifier: Some("./b".into()),
                        },
                    ],
                )],
            ),
            (
                "a.ts".to_string(),
                vec![frag(
                    "aRouter",
                    vec![TrpcRouterEntry::Leaf {
                        key: "x".into(),
                        verb: "QUERY".into(),
                        line: 1,
                    }],
                )],
            ),
            (
                "b.ts".to_string(),
                vec![frag(
                    "bRouter",
                    vec![TrpcRouterEntry::Leaf {
                        key: "y".into(),
                        verb: "MUTATION".into(),
                        line: 2,
                    }],
                )],
            ),
        ];
        let out = compose_trpc_provides(
            fragments,
            resolver(&[("./a", "r.ts", "a.ts"), ("./b", "r.ts", "b.ts")]),
        );
        assert_eq!(
            keys(&out),
            vec![
                ("MUTATION y".to_string(), "b.ts".to_string(), 2),
                ("QUERY x".to_string(), "a.ts".to_string(), 1),
            ]
        );
    }

    #[test]
    fn unresolvable_ref_is_skipped_sibling_leaf_survives() {
        let fragments = vec![(
            "a.ts".to_string(),
            vec![frag(
                "appRouter",
                vec![
                    TrpcRouterEntry::Ref {
                        key: "missing".into(),
                        ident: "ghost".into(),
                        specifier: Some("./ghost".into()),
                    },
                    TrpcRouterEntry::Leaf {
                        key: "ok".into(),
                        verb: "QUERY".into(),
                        line: 1,
                    },
                ],
            )],
        )];
        // resolver answers nothing -> `./ghost` never resolves; `ghost` also names no known fragment even
        // if it did.
        let out = compose_trpc_provides(fragments, no_resolver());
        assert_eq!(
            keys(&out),
            vec![("QUERY ok".to_string(), "a.ts".to_string(), 1)]
        );
    }

    #[test]
    fn self_referencing_cycle_is_guarded_without_infinite_recursion() {
        let fragments = vec![(
            "app.ts".to_string(),
            vec![
                frag(
                    "app",
                    vec![TrpcRouterEntry::Ref {
                        key: "a".into(),
                        ident: "a".into(),
                        specifier: None,
                    }],
                ),
                frag(
                    "a",
                    vec![
                        TrpcRouterEntry::Leaf {
                            key: "x".into(),
                            verb: "QUERY".into(),
                            line: 5,
                        },
                        // Cycles back to itself — must be skipped, not re-composed.
                        TrpcRouterEntry::Ref {
                            key: "loop".into(),
                            ident: "a".into(),
                            specifier: None,
                        },
                    ],
                ),
            ],
        )];
        let out = compose_trpc_provides(fragments, no_resolver());
        assert_eq!(
            keys(&out),
            vec![("QUERY a.x".to_string(), "app.ts".to_string(), 5)]
        );
    }

    #[test]
    fn composition_is_deterministic_under_input_order_reshuffling() {
        let build = |reversed: bool| {
            let mut fragments = vec![
                (
                    "trpc.ts".to_string(),
                    vec![frag(
                        "appRouter",
                        vec![TrpcRouterEntry::Ref {
                            key: "viewer".into(),
                            ident: "viewerRouter".into(),
                            specifier: Some("./viewer".into()),
                        }],
                    )],
                ),
                (
                    "viewer.ts".to_string(),
                    vec![frag(
                        "viewerRouter",
                        vec![TrpcRouterEntry::Leaf {
                            key: "me".into(),
                            verb: "QUERY".into(),
                            line: 1,
                        }],
                    )],
                ),
            ];
            if reversed {
                fragments.reverse();
            }
            fragments
        };
        let resolve = || resolver(&[("./viewer", "trpc.ts", "viewer.ts")]);
        let out1 = compose_trpc_provides(build(false), resolve());
        let out2 = compose_trpc_provides(build(true), resolve());
        assert_eq!(keys(&out1), keys(&out2));
    }
}

#[cfg(test)]
mod controller_prefix_compose_tests {
    //! Coverage for `compose_controller_prefix_provides`: literal resolution against the merged const
    //! map, the never-guess unresolved warning (aggregated per `(file, prefix_ref)`, singular/plural
    //! wording), a resolved and an unresolved controller side by side, and determinism.
    use super::*;
    use zzop_core::ControllerPrefixRouteFragment;

    fn frag(
        prefix_ref: &str,
        verb: &str,
        path: &str,
        line: u32,
        symbol: &str,
    ) -> ControllerPrefixRouteFragment {
        ControllerPrefixRouteFragment {
            body: None,
            prefix_ref: prefix_ref.to_string(),
            verb: verb.to_string(),
            path: path.to_string(),
            line,
            symbol: Some(symbol.to_string()),
        }
    }

    fn consts(entries: &[(&str, &str)]) -> HashMap<String, String> {
        entries
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn resolved_prefix_ref_composes_a_joined_provide() {
        let fragments = vec![(
            "src/asset.controller.ts".to_string(),
            vec![
                frag("RouteKey.Asset", "GET", ":id", 3, "getById"),
                frag("RouteKey.Asset", "DELETE", "", 6, "remove"),
            ],
        )];
        let mut warnings = Vec::new();
        let out = compose_controller_prefix_provides(
            fragments,
            &consts(&[("RouteKey.Asset", "assets")]),
            &mut warnings,
        );
        let keys: Vec<&str> = out.iter().map(|p| p.key.as_str()).collect();
        assert_eq!(keys, vec!["DELETE /assets", "GET /assets/{}"]);
        assert!(warnings.is_empty());
        let get = out.iter().find(|p| p.key == "GET /assets/{}").unwrap();
        assert_eq!(get.file, "src/asset.controller.ts");
        assert_eq!(get.line, 3);
        assert_eq!(get.symbol.as_deref(), Some("getById"));
    }

    #[test]
    fn unresolved_prefix_ref_drops_its_routes_and_warns_once_per_file_and_ref() {
        let fragments = vec![(
            "controller.ts".to_string(),
            vec![
                frag("RouteKey.Asset", "GET", "a", 1, "a"),
                frag("RouteKey.Asset", "GET", "b", 2, "b"),
            ],
        )];
        let mut warnings = Vec::new();
        let out = compose_controller_prefix_provides(fragments, &consts(&[]), &mut warnings);
        assert!(out.is_empty(), "never guess an unresolved prefix: {out:?}");
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("RouteKey.Asset"));
        assert!(warnings[0].contains("controller.ts"));
        assert!(warnings[0].contains("2 routes"));
    }

    #[test]
    fn singular_route_count_uses_singular_wording() {
        let fragments = vec![(
            "controller.ts".to_string(),
            vec![frag("RouteKey.Asset", "GET", "a", 1, "a")],
        )];
        let mut warnings = Vec::new();
        compose_controller_prefix_provides(fragments, &consts(&[]), &mut warnings);
        assert!(warnings[0].contains("1 route "), "{warnings:?}");
        assert!(!warnings[0].contains("1 routes"), "{warnings:?}");
    }

    #[test]
    fn resolved_and_unresolved_controllers_are_independent() {
        let fragments = vec![
            (
                "resolved.controller.ts".to_string(),
                vec![frag("RouteKey.Asset", "GET", "a", 1, "a")],
            ),
            (
                "unresolved.controller.ts".to_string(),
                vec![frag("RouteKey.Missing", "GET", "b", 1, "b")],
            ),
        ];
        let mut warnings = Vec::new();
        let out = compose_controller_prefix_provides(
            fragments,
            &consts(&[("RouteKey.Asset", "assets")]),
            &mut warnings,
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key, "GET /assets/a");
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("RouteKey.Missing"));
    }

    #[test]
    fn output_is_deterministic_across_input_order() {
        let build = |rev: bool| {
            let mut v = vec![
                (
                    "a.controller.ts".to_string(),
                    vec![frag("RouteKey.A", "GET", "a", 1, "a")],
                ),
                (
                    "b.controller.ts".to_string(),
                    vec![frag("RouteKey.B", "GET", "b", 1, "b")],
                ),
            ];
            if rev {
                v.reverse();
            }
            v
        };
        let c = consts(&[("RouteKey.A", "a-prefix"), ("RouteKey.B", "b-prefix")]);
        let mut w1 = Vec::new();
        let mut w2 = Vec::new();
        let out1 = compose_controller_prefix_provides(build(false), &c, &mut w1);
        let out2 = compose_controller_prefix_provides(build(true), &c, &mut w2);
        let view = |v: &[IoProvide]| -> Vec<(String, String, u32)> {
            v.iter()
                .map(|p| (p.key.clone(), p.file.clone(), p.line))
                .collect()
        };
        assert_eq!(view(&out1), view(&out2));
    }

    #[test]
    fn body_shape_is_carried_through_onto_the_composed_provide() {
        // `ControllerPrefixRouteFragment.body` (`body-shape-v1`) must survive the prefix-ref join
        // unchanged — `resolve_provide_body_refs` resolves its `dto_ref` in a LATER pass, over whatever
        // `io_provides` holds by then, this composer included.
        let mut with_body = frag("RouteKey.Asset", "POST", "", 1, "create");
        with_body.body = Some(zzop_core::ProvideBodyShape {
            sub_key: None,
            dto_ref: Some("CreateAssetDto".to_string()),
            fields: Vec::new(),
            complete: false,
        });
        let fragments = vec![("asset.controller.ts".to_string(), vec![with_body])];
        let mut warnings = Vec::new();
        let out = compose_controller_prefix_provides(
            fragments,
            &consts(&[("RouteKey.Asset", "assets")]),
            &mut warnings,
        );
        assert_eq!(out.len(), 1);
        let body = out[0].body.as_ref().expect("body carried through");
        assert_eq!(body.dto_ref.as_deref(), Some("CreateAssetDto"));
    }
}

#[cfg(test)]
mod merge_const_map_fragments_tests {
    use super::*;

    #[test]
    fn first_writer_wins_by_sorted_rel_regardless_of_input_order() {
        let mut a: HashMap<String, String> = HashMap::new();
        a.insert("K".to_string(), "from-a".to_string());
        let mut z: HashMap<String, String> = HashMap::new();
        z.insert("K".to_string(), "from-z".to_string());

        let in_order = vec![
            ("a.ts".to_string(), a.clone()),
            ("z.ts".to_string(), z.clone()),
        ];
        let reversed = vec![("z.ts".to_string(), z), ("a.ts".to_string(), a)];

        assert_eq!(
            merge_const_map_fragments(&in_order).get("K"),
            merge_const_map_fragments(&reversed).get("K")
        );
        assert_eq!(
            merge_const_map_fragments(&in_order)
                .get("K")
                .map(String::as_str),
            Some("from-a")
        );
    }
}

#[cfg(test)]
mod resolve_provide_body_refs_tests {
    //! Coverage for `resolve_provide_body_refs`: successful ref resolution (fields/complete copied,
    //! `dto_ref` cleared), a conflicting duplicate class name poisoning that name (with an aggregated
    //! warning), a missing ref dropping the whole `body` (with an aggregated warning), and an identical
    //! duplicate across 2 files resolving normally with no warning.
    use super::*;
    use zzop_core::{ClassShapeFragment, ProvideBodyField, ProvideBodyShape};

    fn class(name: &str, fields: &[(&str, bool)], complete: bool) -> ClassShapeFragment {
        ClassShapeFragment {
            name: name.to_string(),
            fields: fields
                .iter()
                .map(|(n, optional)| ProvideBodyField {
                    name: n.to_string(),
                    optional: *optional,
                })
                .collect(),
            complete,
        }
    }

    fn provide_with_ref(file: &str, line: u32, dto_ref: &str) -> IoProvide {
        IoProvide {
            body: Some(ProvideBodyShape {
                sub_key: None,
                dto_ref: Some(dto_ref.to_string()),
                fields: Vec::new(),
                complete: false,
            }),
            kind: "http".to_string(),
            key: "POST /api/users".to_string(),
            file: file.to_string(),
            line,
            symbol: None,
        }
    }

    #[test]
    fn resolved_ref_copies_fields_and_complete_and_clears_dto_ref() {
        let mut provides = vec![provide_with_ref("controller.ts", 10, "CreateUserDto")];
        let class_shapes = vec![(
            "dto.ts".to_string(),
            vec![class(
                "CreateUserDto",
                &[("name", false), ("nickname", true)],
                true,
            )],
        )];
        let mut warnings = Vec::new();
        resolve_provide_body_refs(&mut provides, class_shapes, &mut warnings);
        assert!(warnings.is_empty());
        let body = provides[0].body.as_ref().unwrap();
        assert_eq!(body.dto_ref, None);
        assert!(body.complete);
        let names: Vec<&str> = body.fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, vec!["name", "nickname"]);
        assert!(!body.fields[0].optional);
        assert!(body.fields[1].optional);
    }

    #[test]
    fn conflicting_duplicate_class_shape_poisons_the_name_and_warns_on_both_sides() {
        // Two warnings are expected: one aggregated warning naming the class + conflicting files (the
        // MERGE step's honest-degrade), and one aggregated warning naming the dropped provide(s) (the
        // PROVIDE-resolution step's honest-degrade) — distinct concerns, both disclosed.
        let mut provides = vec![provide_with_ref("controller.ts", 10, "CreateUserDto")];
        let class_shapes = vec![
            (
                "a.ts".to_string(),
                vec![class("CreateUserDto", &[("name", false)], true)],
            ),
            (
                "b.ts".to_string(),
                vec![class(
                    "CreateUserDto",
                    &[("name", false), ("email", false)],
                    true,
                )],
            ),
        ];
        let mut warnings = Vec::new();
        resolve_provide_body_refs(&mut provides, class_shapes, &mut warnings);
        assert!(provides[0].body.is_none(), "poisoned ref drops the body");
        assert_eq!(warnings.len(), 2);
        let conflict_warning = warnings
            .iter()
            .find(|w| w.contains("conflicting"))
            .expect("a conflicting-shape warning");
        assert!(conflict_warning.contains("CreateUserDto"));
        assert!(conflict_warning.contains("a.ts"));
        assert!(conflict_warning.contains("b.ts"));
        let drop_warning = warnings
            .iter()
            .find(|w| w.contains("could not resolve"))
            .expect("a dropped-provide warning");
        assert!(drop_warning.contains("CreateUserDto"));
        assert!(drop_warning.contains("controller.ts"));
    }

    #[test]
    fn unreferenced_conflicting_class_shape_stays_silent() {
        // Class-shape fragments cover EVERY class declaration, so same-name/different-shape
        // non-DTO classes (`Config`, `Options`, ...) are common and legitimate — a collision no
        // provide's `dto_ref` references must not warn (that would disclose a drop that never
        // happened: a phantom disclosure).
        let mut provides = vec![provide_with_ref("controller.ts", 10, "CreateUserDto")];
        let class_shapes = vec![
            (
                "a.ts".to_string(),
                vec![
                    class("CreateUserDto", &[("name", false)], true),
                    class("Options", &[("a", false)], true),
                ],
            ),
            (
                "b.ts".to_string(),
                vec![class("Options", &[("b", false)], true)],
            ),
        ];
        let mut warnings = Vec::new();
        resolve_provide_body_refs(&mut provides, class_shapes, &mut warnings);
        assert!(
            warnings.is_empty(),
            "unreferenced collision must not warn: {warnings:?}"
        );
        let body = provides[0].body.as_ref().unwrap();
        assert_eq!(
            body.dto_ref, None,
            "the referenced ref still resolves normally"
        );
    }

    #[test]
    fn identical_duplicate_class_shape_across_two_files_resolves_without_warning() {
        let mut provides = vec![provide_with_ref("controller.ts", 10, "CreateUserDto")];
        let class_shapes = vec![
            (
                "a.ts".to_string(),
                vec![class("CreateUserDto", &[("name", false)], true)],
            ),
            (
                "b.ts".to_string(),
                vec![class("CreateUserDto", &[("name", false)], true)],
            ),
        ];
        let mut warnings = Vec::new();
        resolve_provide_body_refs(&mut provides, class_shapes, &mut warnings);
        assert!(warnings.is_empty());
        let body = provides[0].body.as_ref().unwrap();
        assert_eq!(body.dto_ref, None);
    }

    #[test]
    fn missing_ref_drops_the_whole_body_and_warns_with_a_count() {
        let mut provides = vec![
            provide_with_ref("controller.ts", 10, "CreateUserDto"),
            provide_with_ref("controller.ts", 20, "CreateUserDto"),
        ];
        let mut warnings = Vec::new();
        resolve_provide_body_refs(&mut provides, Vec::new(), &mut warnings);
        assert!(provides[0].body.is_none());
        assert!(provides[1].body.is_none());
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("CreateUserDto"));
        assert!(warnings[0].contains("controller.ts"));
        assert!(warnings[0].contains("2 provides"));
    }

    #[test]
    fn provide_with_no_dto_ref_is_left_untouched() {
        let mut provides = vec![IoProvide {
            body: Some(ProvideBodyShape {
                sub_key: None,
                dto_ref: None,
                fields: vec![ProvideBodyField {
                    name: "name".to_string(),
                    optional: false,
                }],
                complete: true,
            }),
            kind: "http".to_string(),
            key: "POST /api/users".to_string(),
            file: "controller.ts".to_string(),
            line: 1,
            symbol: None,
        }];
        let mut warnings = Vec::new();
        resolve_provide_body_refs(&mut provides, Vec::new(), &mut warnings);
        assert!(warnings.is_empty());
        assert!(provides[0].body.is_some());
    }
}

#[cfg(test)]
mod client_base_prefix_tests {
    //! Coverage for `apply_client_base_prefixes`: the single-prefix apply, client-scoping (a
    //! differently- or un-tagged consume is untouched), the external/absolute-URL and unresolved
    //! never-touch gates, non-`http`-kind gating, the conflicting-sentinels honest degrade, the
    //! same-path duplicate-sentinel no-warning case, and the deliberate double-prefix idempotence
    //! pin.
    use super::*;

    fn sentinel(path: &str, client: &str, file: &str, line: u32) -> IoConsume {
        IoConsume {
            client: Some(client.to_string()),
            body: None,
            kind: "client-base-prefix".to_string(),
            key: Some(path.to_string()),
            file: file.to_string(),
            line,
            raw: None,
            method: None,
        }
    }

    fn http_consume(key: &str, client: Option<&str>, file: &str, line: u32) -> IoConsume {
        IoConsume {
            client: client.map(str::to_string),
            body: None,
            kind: "http".to_string(),
            key: Some(key.to_string()),
            file: file.to_string(),
            line,
            raw: None,
            method: None,
        }
    }

    fn unresolved_consume(client: Option<&str>, raw: &str, file: &str, line: u32) -> IoConsume {
        IoConsume {
            client: client.map(str::to_string),
            body: None,
            kind: "http".to_string(),
            key: None,
            file: file.to_string(),
            line,
            raw: Some(raw.to_string()),
            method: Some("GET".to_string()),
        }
    }

    #[test]
    fn single_prefix_rewrites_axios_tagged_http_consumes_and_strips_the_sentinel() {
        let mut consumes = vec![
            sentinel("/api", "axios", "src/bootstrap.ts", 3),
            http_consume("GET /users", Some("axios"), "src/api/users.ts", 10),
        ];
        let mut warnings = Vec::new();
        apply_client_base_prefixes(&mut consumes, &mut warnings);
        assert_eq!(consumes.len(), 1);
        assert_eq!(consumes[0].key.as_deref(), Some("GET /api/users"));
        assert!(warnings.is_empty());
        assert!(consumes.iter().all(|c| c.kind != "client-base-prefix"));
    }

    #[test]
    fn non_axios_tagged_consume_is_untouched() {
        // `client: None` and `client: Some("fetch")` both must be left alone — the prefix is
        // scoped to the SAME client tag the sentinel names.
        let mut consumes = vec![
            sentinel("/api", "axios", "src/bootstrap.ts", 3),
            http_consume("GET /users", None, "src/a.ts", 1),
            http_consume("GET /orders", Some("fetch"), "src/b.ts", 2),
        ];
        let mut warnings = Vec::new();
        apply_client_base_prefixes(&mut consumes, &mut warnings);
        assert_eq!(consumes.len(), 2);
        assert_eq!(consumes[0].key.as_deref(), Some("GET /users"));
        assert_eq!(consumes[1].key.as_deref(), Some("GET /orders"));
    }

    #[test]
    fn absolute_url_key_is_untouched() {
        let mut consumes = vec![
            sentinel("/api", "axios", "src/bootstrap.ts", 3),
            http_consume("GET https://x.io/users", Some("axios"), "src/a.ts", 1),
        ];
        let mut warnings = Vec::new();
        apply_client_base_prefixes(&mut consumes, &mut warnings);
        assert_eq!(consumes.len(), 1);
        assert_eq!(consumes[0].key.as_deref(), Some("GET https://x.io/users"));
    }

    #[test]
    fn unresolved_consume_key_is_untouched() {
        let mut consumes = vec![
            sentinel("/api", "axios", "src/bootstrap.ts", 3),
            unresolved_consume(Some("axios"), "axios.get(url)", "src/a.ts", 1),
        ];
        let mut warnings = Vec::new();
        apply_client_base_prefixes(&mut consumes, &mut warnings);
        assert_eq!(consumes.len(), 1);
        assert_eq!(consumes[0].key, None);
        assert_eq!(consumes[0].raw.as_deref(), Some("axios.get(url)"));
    }

    #[test]
    fn conflicting_sentinels_apply_nothing_and_warn_once_naming_both() {
        let mut consumes = vec![
            sentinel("/api", "axios", "src/a.ts", 1),
            sentinel("/v2", "axios", "src/b.ts", 2),
            http_consume("GET /users", Some("axios"), "src/c.ts", 10),
        ];
        let mut warnings = Vec::new();
        apply_client_base_prefixes(&mut consumes, &mut warnings);
        assert_eq!(consumes.len(), 1);
        assert_eq!(consumes[0].key.as_deref(), Some("GET /users")); // unchanged — never guess
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("axios"));
        assert!(warnings[0].contains("/api"));
        assert!(warnings[0].contains("src/a.ts:1"));
        assert!(warnings[0].contains("/v2"));
        assert!(warnings[0].contains("src/b.ts:2"));
        assert!(consumes.iter().all(|c| c.kind != "client-base-prefix"));
    }

    #[test]
    fn duplicate_sentinels_with_the_same_path_apply_once_with_no_warning() {
        let mut consumes = vec![
            sentinel("/api", "axios", "src/a.ts", 1),
            sentinel("/api", "axios", "src/b.ts", 2),
            http_consume("GET /users", Some("axios"), "src/c.ts", 10),
        ];
        let mut warnings = Vec::new();
        apply_client_base_prefixes(&mut consumes, &mut warnings);
        assert_eq!(consumes.len(), 1);
        assert_eq!(consumes[0].key.as_deref(), Some("GET /api/users"));
        assert!(warnings.is_empty());
    }

    #[test]
    fn non_http_kind_with_axios_tag_is_untouched() {
        // Shouldn't exist in practice (only `http` consumes carry a client tag today), but the
        // gate must be on `kind`, not merely on the client tag being present.
        let mut consumes = vec![
            sentinel("/api", "axios", "src/a.ts", 1),
            IoConsume {
                client: Some("axios".to_string()),
                body: None,
                kind: "trpc".to_string(),
                key: Some("GET users".to_string()),
                file: "src/c.ts".to_string(),
                line: 1,
                raw: None,
                method: None,
            },
        ];
        let mut warnings = Vec::new();
        apply_client_base_prefixes(&mut consumes, &mut warnings);
        assert_eq!(consumes.len(), 1);
        assert_eq!(consumes[0].key.as_deref(), Some("GET users"));
    }

    #[test]
    fn prefix_already_present_in_the_path_is_still_prepended() {
        // Deliberate: the axios runtime really does double it (`baseURL: '/api'` + a call site
        // that itself already resolves to `/api/users` -> the wire request really goes to
        // `/api/api/users`). Pins the semantic rather than trying to detect/dedupe it.
        let mut consumes = vec![
            sentinel("/api", "axios", "src/a.ts", 1),
            http_consume("GET /api/users", Some("axios"), "src/c.ts", 10),
        ];
        let mut warnings = Vec::new();
        apply_client_base_prefixes(&mut consumes, &mut warnings);
        assert_eq!(consumes[0].key.as_deref(), Some("GET /api/api/users"));
    }
}
