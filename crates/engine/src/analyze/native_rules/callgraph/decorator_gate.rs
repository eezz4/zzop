//! Helpers for `run_callgraph_rules`'s decorator-guard evidence gate — split out of `mod.rs` purely to
//! stay under the repo's per-file line cap; every item here is `pub(super)`, used only by `callgraph::mod`.

use std::collections::HashSet;

use zzop_core::{is_pack_enabled, IoProvide, Matcher, SourceSymbol};

use crate::EngineConfig;

use super::python_guard;

/// Everything that feeds `run_callgraph_rules`' one framework-neutral `(file, line)` decorator-guard set,
/// merged in one place. Split out of `mod.rs` for the same line-cap reason as the rest of this file —
/// the ORDER is load-bearing and documented at each step below, so it is kept as one function rather
/// than scattered back across the caller.
///
/// Producers, in application order:
/// 1. **Spring method security** — gathered by the caller's Java loop into `java_decorator_guarded`.
/// 2. **FastAPI `Depends`** — already anchored on the route decorator's own `(file, line)`, so it merges
///    exactly like Spring's.
/// 3. **DRF `permission_classes`** — applied by NAME, which is why it comes after the line-anchored ones:
///    it needs `io_provides` rather than a line.
/// 4. **NestJS `@UseGuards`** — read from the TS texts the caller already has in memory (no extra I/O).
/// 5. **NestJS route-scoped middleware** — matched by (method, path) PATTERN, not a line.
/// 6. **Spring Security global posture** — applied only when EXACTLY one exists tree-wide (else
///    config-vs-config scoping is ambiguous) and SCOPED to that config's own source root, so it never
///    false-clears a sibling module's open routes.
///
/// Rust has no entry here on purpose: its guard evidence is a TYPE in the handler signature, which the
/// BFS already reaches as a real graph edge (`zzop_parser_rust::parse_extractor_guards`), so it needs no
/// side-channel at all.
/// The TypeScript half of the decorator-guard evidence, gathered file by file as the caller reads
/// each source and dropped along with the text.
///
/// This type exists so that `callgraph::run` does not have to keep every TS file's source alive to the
/// end of the pass — see the memory contract at its read loop. The three extractors folded in here were
/// the only reason that map existed, and each of them looks at exactly one file, so the pass keeps
/// their (small) output instead of their (unbounded) input.
#[derive(Default)]
pub(super) struct TsGuardEvidence {
    /// `(file, line)` pairs from `extract_controller_guarded_lines`.
    guarded_lines: Vec<(String, u32)>,
    /// Nest `forRoutes` patterns. Consumed with `.any(..)`, so collection order cannot reach a verdict.
    forroutes: Vec<zzop_parser_typescript::ForRoutesPattern>,
    /// `(file, prefix)` from `extract_global_prefix_marker`. Kept as a list rather than a first hit
    /// because the winner is the LOWEST path, and this loop meets files in walk order.
    prefix_candidates: Vec<(String, String)>,
}

impl TsGuardEvidence {
    /// Folds one file's already-extracted evidence in.
    ///
    /// This used to take the file's TEXT and run three extractors on it, which is why the pass that
    /// called it had to read every source. The extractors moved to `pipeline::fresh::call_graph`
    /// (review ledger V108); what is left here is the fold, which is all this type ever wanted.
    pub(super) fn absorb(&mut self, rel: &str, facts: &zzop_core::callgraph::CallGraphFacts) {
        #[allow(
            clippy::iter_over_hash_type,
            reason = "the lines are pushed onto `guarded_lines`, which the assembler drains into a `HashSet` of `(file, line)` — a commutative fold, so the visit order of that extractor cannot reach a verdict"
        )]
        for line in &facts.controller_guarded_lines {
            self.guarded_lines.push((rel.to_string(), *line));
        }
        self.forroutes.extend(facts.nest_forroutes.iter().cloned());
        if let Some(p) = &facts.global_prefix {
            self.prefix_candidates.push((rel.to_string(), p.clone()));
        }
    }
}

pub(super) fn assemble_decorator_guarded(
    java_decorator_guarded: HashSet<(String, u32)>,
    python_guards: &python_guard::PythonGuards,
    spring_postures: &[(String, zzop_parser_java_21::SpringSecurityPosture)],
    ts_guards: &TsGuardEvidence,
    io_provides: &[IoProvide],
    all_symbols: &[SourceSymbol],
    java_source_root: Option<&str>,
) -> HashSet<(String, u32)> {
    let mut guarded = java_decorator_guarded;
    guarded.extend(python_guards.guarded_lines.iter().cloned());
    python_guard::apply_django_view_guards(
        io_provides,
        &python_guards.guarded_view_classes,
        all_symbols,
        &mut guarded,
    );
    guarded.extend(ts_guards.guarded_lines.iter().cloned());
    apply_nest_forroutes_guards(ts_guards, io_provides, &mut guarded);
    if let [(config_file, posture)] = spring_postures {
        let app_root = spring_app_root(config_file, java_source_root);
        for p in io_provides.iter().filter(|p| {
            p.kind == "http" && p.file.ends_with(".java") && p.file.starts_with(app_root)
        }) {
            let Some((method, path)) = p.key.split_once(' ') else {
                continue;
            };
            if posture.route_is_authenticated(method, path) {
                guarded.insert((p.file.clone(), p.line));
            }
        }
    }
    guarded
}

/// Whether at least one loaded+enabled DSL pack has an `IoScan` rule that would actually READ the
/// decorator-guard evidence `run_callgraph_rules` produces (`attr_present`/`attr_absent`, the vocab-free
/// `AttributeStore` gate `assemble/rules/io_scan.rs`'s `mint_auth_guarded` feeds — e.g. the shipped http
/// pack's `protected-path-no-auth-evidence`, post-A2-migration). Gated identically to `io_scan::run`'s own
/// pack/rule enablement
/// (`is_enabled` at the pack level, then `pipeline::gate_pack_rules` per rule) so this predicate can never
/// disagree with what actually runs. This is the OTHER consumer `run_callgraph_rules`'s
/// `need_decorator_guarded` ORs against `run_mutating_no_auth` — see that binding's doc for why.
pub(super) fn packs_read_io_scan_attrs(config: &EngineConfig) -> bool {
    config
        .packs
        .iter()
        .filter(|p| is_pack_enabled(&config.rule_config, &p.id))
        .map(|p| crate::pipeline::gate_pack_rules(p, config))
        .any(|gated| {
            gated.rules.iter().any(|r| {
                matches!(&r.matcher, Matcher::IoScan(m) if m.attr_present.is_some() || m.attr_absent.is_some())
            })
        })
}

/// Whether a route provide's path (leading-slash, `http_interface_key`-normalized, and already carrying
/// the app's NestJS global prefix if one exists — `/api/articles/{}/comments`) is EXACTLY the route a
/// NestJS `forRoutes` PATTERN covers (controller-relative, no leading slash, no global prefix —
/// `articles/{}/comments`). The pattern is reconciled to the provide's key space by prepending
/// `global_prefix` (when a literal one was found) and comparing for EQUALITY — not a suffix match, which
/// would over-clear (a `{path:'articles'}` pattern must not exempt an unrelated `/api/admin/articles`
/// route in another module). Both sides already share the `{}` param normalization. When `global_prefix`
/// is `None` (no `setGlobalPrefix`, or a non-literal one that can't be read), the pattern is matched
/// unprefixed; if the app truly has a prefix we failed to read, the exemption is simply MISSED (the
/// finding stays) — never an over-clear, the safe direction for a security rule.
pub(super) fn forroutes_path_matches(
    provide_path: &str,
    pattern: &str,
    global_prefix: Option<&str>,
) -> bool {
    let pat = pattern.trim_start_matches('/');
    let expected = match global_prefix {
        Some(p) if !p.trim_matches('/').is_empty() => format!("/{}/{}", p.trim_matches('/'), pat),
        _ => format!("/{pat}"),
    };
    provide_path == expected
}

/// The Java source-root prefix a Spring security config governs — everything up to and including the first
/// `src_root` segment, so a posture only exempts routes in its OWN module. A monorepo module lives at
/// `<module>/src/main/java/...`, so `service-a`'s config yields prefix `service-a/src/main/java/` and can
/// never match `service-b/src/main/java/...`. When the config isn't under a recognizable source root
/// (unusual layout), falls back to the config file's own directory — the most conservative scope (only
/// same-directory routes), never the whole tree.
///
/// `src_root` is the run's declared value (`vocabulary.javaSourceRoot`): the Maven/Gradle layout is a
/// convention a project can relocate, so where its sources live is a fact only the project can state.
/// `None` — no declaration — takes the same conservative branch as an unrecognizable layout, because
/// "we were not told where sources live" and "sources are not where we expected" leave the same thing
/// unknown, and both must narrow the exemption rather than widen it.
pub(super) fn spring_app_root<'a>(config_file: &'a str, src_root: Option<&str>) -> &'a str {
    if let Some(idx) = src_root.and_then(|r| config_file.find(r).map(|i| i + r.len())) {
        &config_file[..idx]
    } else {
        match config_file.rfind('/') {
            Some(i) => &config_file[..=i],
            None => "",
        }
    }
}

/// NestJS route-scoped auth middleware: `consumer.apply(AuthX).forRoutes({path, method})` in a module
/// names its covered routes by (method, path) PATTERN, not a `(file, line)`. Matches each pattern against
/// the actual route provides and exempts every match by its OWN registration line, so the result merges
/// into the same framework-neutral `decorator_guarded` set every other producer feeds.
///
/// The app's NestJS global prefix (`app.setGlobalPrefix('api')`), if any, is prepended before matching: a
/// controller route provide's key already carries it (applied at assembly) but a `forRoutes` path is
/// written WITHOUT it. A non-literal/absent prefix leaves it `None` (exact match against the unprefixed
/// pattern) — a miss then only fails to exempt, never over-exempts.
pub(super) fn apply_nest_forroutes_guards(
    ts_guards: &TsGuardEvidence,
    io_provides: &[zzop_core::IoProvide],
    decorator_guarded: &mut std::collections::HashSet<(String, u32)>,
) {
    let forroutes = &ts_guards.forroutes;
    if forroutes.is_empty() {
        return;
    }
    // LOWEST path wins, and the tie-break is the whole point: a monorepo where two apps each call
    // `setGlobalPrefix` with a DIFFERENT literal would otherwise resolve to whichever one the walk
    // reached first — flipping a `forRoutes` exemption (matched by path EQUALITY below) on and off
    // between runs with identical input. The sibling Python guard phase sorts for the same reason;
    // picking the lowest path is arbitrary but STABLE, which is the property that matters.
    //
    // This used to sort the keys of a `HashMap` of file texts. It now sorts the few files that actually
    // carry a prefix marker, which is the same winner over a smaller set — the map itself was the
    // memory cost this pass no longer pays.
    let global_prefix: Option<String> = ts_guards
        .prefix_candidates
        .iter()
        .min_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, prefix)| prefix.clone());
    for p in io_provides.iter().filter(|p| p.kind == "http") {
        let Some((method, path)) = p.key.split_once(' ') else {
            continue;
        };
        let covered = forroutes.iter().any(|(m, pat)| {
            (m == "*" || m == method) && forroutes_path_matches(path, pat, global_prefix.as_deref())
        });
        if covered {
            decorator_guarded.insert((p.file.clone(), p.line));
        }
    }
}
