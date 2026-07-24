//! `super::run` (phase 4)'s own last sub-phase: the whole-tree `Matcher::IoScan` DSL pass, the native-path
//! counterpart to the per-file `LineScan`/`MethodScan`/`SymbolScan` evaluation the fused pass already ran.
//! Called AFTER `run_callgraph_rules` so its `decorator_guarded` evidence exists, and mints it into the
//! `AttributeStore` an `IoScan` rule's `attr_present`/`attr_absent` gate can see — see [`mint_auth_guarded`]
//! and [`run`].

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::time::Instant;

use zzop_core::{
    eval_pack_io_scan, is_enabled, Attribute, AttributeStore, EntityRef, Finding, IoConsume,
    IoProvide, IoScanTreeContext, Matcher, RulePackDef,
};

use crate::analyze::record_native_timing;
use crate::EngineConfig;

/// Mints an `auth-guarded` [`Attribute`] (`zzop_rules_http::mutating_route_no_auth::AUTH_GUARDED_ATTR`)
/// for every `http` provide whose `(file, line)` is in `decorator_guarded` — the callgraph-BFS pass's own
/// decorator/annotation/middleware-pattern auth evidence (`@PreAuthorize`, `@UseGuards`, `forRoutes`, ...),
/// re-expressed as a route-keyed attribute an `IoScan` rule can gate on. Iterates `io_provides` in their
/// existing (pre-assembly-sort) order — the determinism contract this whole pass follows.
fn mint_auth_guarded(
    io_provides: &[IoProvide],
    decorator_guarded: &BTreeSet<(String, u32)>,
) -> Vec<Attribute> {
    io_provides
        .iter()
        .filter(|p| p.kind == "http")
        .filter(|p| decorator_guarded.contains(&(p.file.clone(), p.line)))
        .map(|p| Attribute {
            target: EntityRef::IoKey {
                kind: p.kind.clone(),
                key: p.key.clone(),
            },
            key: zzop_rules_http::mutating_route_no_auth::AUTH_GUARDED_ATTR.to_string(),
            value: serde_json::Value::Bool(true),
        })
        .collect()
}

/// A lazy, per-file line-text cache backing `IoScanTreeContext::anchor_line` for the native path: reads
/// `root.join(rel)` in full on first request for that file, splits into lines, and serves every later
/// `(file, line)` lookup (any candidate match's anchor-exclude/suppress-marker check) from the cached
/// split — a file is read off disk at most once regardless of how many `IoScan` rules or matching entries
/// touch it. `line` is 1-based; `0` (the lookback above line 1) and a missing/unreadable file both yield
/// `None`, never a match.
struct LineCache<'a> {
    root: &'a std::path::Path,
    cache: RefCell<HashMap<String, Option<Vec<String>>>>,
}

impl<'a> LineCache<'a> {
    fn new(root: &'a std::path::Path) -> Self {
        Self {
            root,
            cache: RefCell::new(HashMap::new()),
        }
    }

    fn line(&self, file: &str, line: u32) -> Option<String> {
        if line == 0 {
            return None;
        }
        let mut cache = self.cache.borrow_mut();
        if !cache.contains_key(file) {
            let lines = std::fs::read_to_string(self.root.join(file))
                .ok()
                .map(|text| text.lines().map(str::to_string).collect());
            cache.insert(file.to_string(), lines);
        }
        cache
            .get(file)
            .and_then(|opt| opt.as_ref())
            .and_then(|v| v.get((line - 1) as usize).cloned())
    }
}

/// Runs every loaded+enabled DSL pack's `IoScan` rules against the whole tree, mirroring the per-file
/// pass's own pack gating EXACTLY (`registry::is_enabled` at the pack level, `pipeline::gate_pack_rules`
/// for a per-rule `"{pack}/{rule}"` id — the same two calls `pipeline::run_file_pass` makes) and the same
/// disable-hint append (`pipeline::findings::append_disable_hints`) every other DSL finding-construction
/// site uses — so an `IoScan` finding is indistinguishable, disable/hint-wise, from a per-file DSL finding.
/// `attribute_store` is first extended with [`mint_auth_guarded`]'s minted evidence — gap-filling within
/// the same target-shape class, though a minted exact `IoKey` outranks a covering `PathScope` by
/// `route_attr`'s specificity rule (see `AttributeStore::extended`'s caveat). `anchor_line` reads real
/// source text via [`LineCache`] — the native path's line/suppress-marker channel is live, unlike
/// envelope mode's (see `envelope::ingest`'s own call).
///
/// COUPLING RESOLVED (A2 of the IoScan projection redesign): `decorator_guarded`
/// (`run_callgraph_rules`'s own doc, `callgraph/mod.rs`) is now produced whenever EITHER consumer needs
/// it — the native `mutating-route-no-auth` rule is enabled, OR some loaded+enabled pack's `IoScan` rule
/// reads `attr_present`/`attr_absent` (`callgraph/decorator_gate.rs`'s `packs_read_io_scan_attrs`,
/// computed from `EngineConfig::packs`) — so disabling the native rule alone no longer empties the
/// minted `auth-guarded` attribute out from under a shipped pack (the http pack's `auth-gates`,
/// post-migration). The native rule's own gating (whether `scan_mutating_route_no_auth` itself runs) is
/// untouched — it still depends solely on `mutating-route-no-auth`'s own enablement. Cost, precisely:
/// within a callgraph invocation the producers reuse text the pass already holds (no extra reads
/// per-producer), BUT a config with every callgraph-family rule off where ONLY a DSL pack reads attrs
/// previously early-returned with zero I/O and now pays the pass's own TS+Java file reads — the price of
/// producing evidence that config actually consumes, not free (see `callgraph/mod.rs`'s
/// `need_decorator_guarded` note).
///
/// Rule-timing/profiling parity: under `EngineConfig::profile_rules` this pass contributes one
/// `"{pack}/{rule}"` entry per `IoScan` rule that ran, into the same `rule_time` accumulator every
/// per-file DSL rule and whole-graph native rule feeds — see [`eval_pack_timed`] for the granularity and
/// the reason the profiled path splits the pack. Mode A (`envelope::ingest`'s own `eval_pack_io_scan`
/// call) needs no counterpart: it emits `rule_timings: None` unconditionally, for every rule class.
pub(super) fn run(
    root: &std::path::Path,
    config: &EngineConfig,
    io_provides: &[IoProvide],
    io_consumes: &[IoConsume],
    attribute_store: &AttributeStore,
    decorator_guarded: &BTreeSet<(String, u32)>,
    rule_time: &mut HashMap<String, (u128, usize)>,
) -> Vec<Finding> {
    let minted = mint_auth_guarded(io_provides, decorator_guarded);
    let augmented = attribute_store.extended(minted);

    let mut gated_packs: Vec<RulePackDef> = config
        .packs
        .iter()
        .filter(|p| is_enabled(&config.rule_config, &p.id))
        .map(|p| crate::pipeline::gate_pack_rules(p, &config.rule_config))
        .collect();

    let line_cache = LineCache::new(root);
    let anchor_line = |file: &str, line: u32| line_cache.line(file, line);
    let ctx = IoScanTreeContext {
        provides: io_provides,
        consumes: io_consumes,
        attrs: &augmented,
        anchor_line: &anchor_line,
    };

    // Same profiled/unprofiled branch `pipeline::findings::eval_packs` uses for the per-file pass
    // (`eval_pack_profiled` vs `eval_pack`): an unprofiled run keeps the exact whole-pack call it always
    // made, so it pays no `Instant::now()` and no pack surgery at all.
    let mut findings = Vec::new();
    if config.profile_rules {
        for pack in &mut gated_packs {
            eval_pack_timed(pack, &ctx, rule_time, &mut findings);
        }
    } else {
        for pack in &gated_packs {
            eval_pack_io_scan(pack, &ctx, &mut findings);
        }
    }
    crate::pipeline::findings::append_disable_hints(&mut findings);
    findings
}

/// Profiled counterpart of the plain `eval_pack_io_scan(pack, ..)` call above: evaluates ONE `IoScan`
/// rule at a time so each gets its own `"{pack}/{rule}"` entry in `rule_time`, the exact key shape and
/// accumulator `record_native_timing`'s callers (and the per-file DSL pass, via `RuleTiming`) already use.
///
/// Granularity is per rule per whole-tree pass — ONE clock read around a rule's entire provides+consumes
/// sweep, never per IO entry — matching `eval_pack_profiled`, which likewise times a rule's whole dispatch
/// rather than its inner iteration.
///
/// WHY the pack is split rather than timed inside the evaluator: `zzop_core` exposes only the whole-pack
/// `eval_pack_io_scan` (its per-rule `eval_io_scan_rule` is private), and the per-file pass's profiled
/// entry point has no io-scan twin. Feeding it a one-rule view of the pack is therefore the only
/// engine-side way to attribute time to a rule id. Findings are byte-identical to the whole-pack call:
/// `eval_pack_io_scan` is a straight in-order loop over `pack.rules` with no cross-rule state, so running
/// the same rules in the same order through it appends the same findings in the same order.
///
/// `pack` is drained (a moved-out rule list, non-`IoScan` rules dropped) instead of cloned per rule —
/// `gated_packs` is this function's own local, dead after the loop, so mutating it costs nothing and
/// avoids an O(rules^2) clone of every pack.
fn eval_pack_timed(
    pack: &mut RulePackDef,
    ctx: &IoScanTreeContext,
    rule_time: &mut HashMap<String, (u128, usize)>,
    findings: &mut Vec<Finding>,
) {
    for rule in std::mem::take(&mut pack.rules) {
        // Non-`IoScan` rules are no-ops here (the per-file pass owns them); timing them would mint a
        // spurious entry for a rule this pass never ran.
        if !matches!(rule.matcher, Matcher::IoScan(_)) {
            continue;
        }
        let id = format!("{}/{}", pack.id, rule.id);
        pack.rules.clear();
        pack.rules.push(rule);
        let before = findings.len();
        let t0 = Instant::now();
        eval_pack_io_scan(pack, ctx, findings);
        record_native_timing(rule_time, Some(t0), &id, findings.len() - before);
    }
}

#[cfg(test)]
mod tests;
