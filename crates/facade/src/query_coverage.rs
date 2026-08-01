//! `queryCoverage` — "how much of THIS tree does zzop actually see?", computed by pure post-processing
//! over an already-produced `analyzeTrees` output. Same contract as [`super::query_file`]: no
//! re-analysis, no cache interaction, one core shared by every host.
//!
//! # The three-value cell rule (2026-07-31 user ruling — the design IS the honesty policy)
//! Every fact this reply carries is one of exactly three kinds, and each kind may only say what its
//! source can back:
//!
//! | kind         | source                    | may say                                    |
//! |--------------|---------------------------|--------------------------------------------|
//! | MEASURED     | this run's output         | "in this tree it was N"                    |
//! | CAPABILITY   | code, independent of runs | "this build can/cannot see X"              |
//! | UNMEASURED   | the schema itself         | "never measured — absence of data, not 0"  |
//!
//! **There is deliberately NO single score field, and one must never be added.** Folding the axes into
//! one number would have to either include the unmeasured axis (recall) — manufacturing a claim — or
//! exclude it, in which case the number gets quoted without its exclusion list and reads as "zzop sees
//! N% of my repo" with the missing axis being exactly the one that matters. The `unmeasured` array is a
//! FIELD, not a caveat sentence, precisely so it cannot be dropped in transit the way prose is.
//!
//! The failures this surface closes were all measured on real trees (2026-07): a 91-file Python tree
//! sat at 3 import edges for months because no output gave a per-extension baseline to read 3 against;
//! and 0 findings under a TypeScript-only recognizer reads as "no bug" when it means "not analyzed" —
//! the per-extension dispatch table is the run-level fact that makes both visible. The per-RULE
//! sightline half of the second failure is the CAPABILITY-kind `blindSpots` cell, which was
//! deliberately absent until it could be DERIVED from rule metadata rather than restated by hand — it
//! now is: [`blind_spots`] crosses `zzop_engine::rule_sightlines` (each declaration living WITH its
//! rule, built from the same pinned claim constants the finding prose uses) with the tree's measured
//! extension mix, so `docs/rules/catalog.md`'s sightline prose is never hand-copied here.
//!
//! # Extensions, not language names
//! Files group by their extension (the tail after the last `.`, lowercased), NOT by a language label.
//! Mapping `rs -> "rust (parser-rust)"` here would require a second copy of the engine's dispatch
//! table, and a facade copy is exactly the kind of shadow table this repo keeps finding stale. The
//! extension is a fact of the tree; which dispatch class its files landed in is a fact of the run;
//! both are derivable with no table at all.

use serde_json::{json, Map, Value};

mod blind_spots;

/// One sentence per dispatch class (plus the per-row `inDepGraph` derived count), shipped in the
/// reply so the vocabulary is self-describing — the same discipline as `query_file`'s
/// `verdictMeaning`, sharing its semantics: `structural` here is `analyzed` there, per file.
fn legend() -> Value {
    json!({
        "structural": "a structural projection exists (symbols and/or dep-graph membership) — the \
                       dispatch class that rules needing structure run on. NOT a per-rule claim: \
                       which declared rules still lack their evidence channel on this tree is \
                       `blindSpots`' axis, so read an empty findings list against that list, not as \
                       clean outright",
        "lexicalOnly": "walked and line-scanned only — no parser in this build claims the extension, \
                        so everything needing symbols, imports or io facts was never evaluated; an \
                        empty findings list does NOT mean clean",
        "degraded": "a parser tried and bailed (syntax error or over sizeCap) — text rules ran, \
                     structural ones did not",
        "inDepGraph": "files of this extension contributing at least one RESOLVED outgoing import \
                       edge (a non-empty source entry in the dep graph). A LOW count against `files` \
                       on a structural extension is the import-resolution blindness signal: the \
                       files were parsed, but their imports did not resolve to in-tree files. NOT a \
                       declared-imports count — the output carries only resolved edges, so how many \
                       imports FAILED to resolve is not derivable from this reply"
    })
}

/// Answers over an `analyzeTrees` output. No query parameters: the whole point is the aggregate view,
/// and a caller wanting one file has `queryFile`.
pub fn query_coverage_json(analysis_json: &str) -> Result<String, String> {
    let analysis: Value =
        serde_json::from_str(analysis_json).map_err(|e| format!("analysis JSON: {e}"))?;
    let trees = analysis
        .get("trees")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            "this analysis has no `trees` — the coverage query runs over an analyzeTrees output"
                .to_string()
        })?;

    // CAPABILITY-kind input, read once per reply: the per-rule sightline declarations compiled into
    // this build (see `blind_spots`'s module doc) — a fact of the code, not of this run.
    let sightlines = zzop_engine::rule_sightlines();
    let mut out = Map::new();
    out.insert(
        "trees".to_string(),
        Value::Array(trees.iter().map(|t| tree_view(t, &sightlines)).collect()),
    );
    out.insert("dispatchMeaning".to_string(), legend());
    out.insert("blindSpotMeaning".to_string(), blind_spots::legend());
    // UNMEASURED cells — a schema position, so no consumer can receive the measured axes without
    // receiving the statement of what was never measured. Fixed content by design: it changes when the
    // capability changes, not per run.
    out.insert(
        "unmeasured".to_string(),
        json!([{
            "axis": "recall",
            "note": "How many of the findings that EXIST in this tree zzop reports has never been \
                     measured on this tree. The committed detection benchmark (cases/) scores zzop's \
                     own labeled corpus, not yours — its numbers do not transfer. This is also why \
                     this reply has no single coverage score: folding measured axes into one number \
                     would present it as an answer to the question this axis leaves open."
        }]),
    );
    serde_json::to_string_pretty(&Value::Object(out)).map_err(|e| e.to_string())
}

/// The per-tree aggregation: every fact here is MEASURED (this run) except `blindSpots`, the one
/// CAPABILITY×MEASURED cross (declared sightlines × this tree's structural extensions), and the
/// `channels` sentences say what each number means for the reader instead of leaving a bare scalar to
/// be misread.
fn tree_view(tree: &Value, sightlines: &[zzop_core::RuleSightline]) -> Value {
    let loc = tree.pointer("/output/ir/loc").and_then(Value::as_object);
    let degraded: Vec<&str> = tree
        .pointer("/output/degraded")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    // Files with structure, gathered ONCE per tree rather than per file — `query_file::verdict_for`
    // scans symbols per call, fine for one target and quadratic for a whole tree.
    let mut structural: std::collections::HashSet<&str> = tree
        .pointer("/output/ir/symbols")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|s| s.get("file").and_then(Value::as_str))
                .collect()
        })
        .unwrap_or_default();
    let dep = tree.pointer("/output/ir/dep").and_then(Value::as_object);
    if let Some(dep) = dep {
        structural.extend(dep.keys().map(String::as_str));
    }

    // ext -> (files, structural, lexical_only, degraded, in_dep_graph). BTreeMap: deterministic
    // output order. `in_dep_graph` counts files with a NON-EMPTY dep source entry — key presence
    // alone is not edge participation (the engine gives every parsed file a dep entry, possibly
    // empty, so counting keys would read "parsed" as "resolved" and hide exactly the sparsity this
    // field exists to show: 91 structural .py files with 2-3 of them resolving any import).
    let mut by_ext: std::collections::BTreeMap<String, (usize, usize, usize, usize, usize)> =
        std::collections::BTreeMap::new();
    // Walked paths under a `.git/` SEGMENT — see `walkNote` below.
    let mut git_walked = 0usize;
    if let Some(loc) = loc {
        for rel in loc.keys() {
            let entry = by_ext.entry(ext_of(rel)).or_default();
            entry.0 += 1;
            if degraded.contains(&rel.as_str()) {
                entry.3 += 1;
            } else if structural.contains(rel.as_str()) {
                entry.1 += 1;
            } else {
                entry.2 += 1;
            }
            if dep
                .and_then(|d| d.get(rel))
                .and_then(Value::as_array)
                .is_some_and(|targets| !targets.is_empty())
            {
                entry.4 += 1;
            }
            if rel.split('/').any(|seg| seg == ".git") {
                git_walked += 1;
            }
        }
    }
    let extensions: Vec<Value> = by_ext
        .iter()
        .map(|(ext, (files, s, l, d, in_dep))| {
            json!({ "ext": ext, "files": files, "structural": s, "lexicalOnly": l, "degraded": d,
                    "inDepGraph": in_dep })
        })
        .collect();
    // The extensions with 1+ structural file — the measured half of the `blindSpots` cross.
    let structural_exts: std::collections::BTreeSet<String> = by_ext
        .iter()
        .filter(|(_, counts)| counts.1 > 0)
        .map(|(ext, _)| ext.clone())
        .collect();

    let census = tree
        .pointer("/output/coverage")
        .cloned()
        .unwrap_or(Value::Null);
    let join_zero = census
        .get("joinContributionZero")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut view = json!({
        "sourceId": tree.get("sourceId").cloned().unwrap_or(Value::Null),
        "extensions": extensions,
        "blindSpots": blind_spots::blind_spots(sightlines, &structural_exts),
        // What the cross above was computed FROM (derived at emit time) — without it an empty
        // `blindSpots` reads the same for "crossed, nothing blind" and "no structural input at all".
        "blindSpotBasis": blind_spots::basis(sightlines.len(), structural_exts.len()),
        // The tree's own engine self-reports, forwarded verbatim — the same field `zzop facts`
        // carries. The framework-silence warnings (e.g. the call-graph coverage gap naming
        // mutating-route-no-auth) live HERE, not in any sightline declaration: that gap is
        // route-conditional and owned by the per-run warning, so a coverage surface that dropped
        // this channel was hiding the one disclosure that covers it (measured 2026-07-31 on a Go
        // tree).
        "warnings": tree.pointer("/output/warnings").cloned().unwrap_or(json!([])),
        // The census verbatim (MEASURED), plus the one sentence its most misread bit needs: a bare
        // `joinContributionZero: true` scalar was shipping since the census landed and the misreading
        // it guards against still required the reader to know the field.
        "census": census,
        // "that contributes io facts" is load-bearing: the output carries NO structured signal of
        // whether adapter overlays were applied (a clean application produces no warning and
        // `OverlayApplication` never serializes), so this sentence cannot branch on "an overlay is
        // already loaded" — and the generic "an adapter overlay restores visibility" was measured
        // misleading on a tree that already carried an import-alias overlay (no io) and stayed
        // join-blind. The reword makes the sentence true in both worlds: it names WHAT the overlay
        // must contribute, so it can never read as "add any overlay".
        "joinVisibility": if join_zero {
            "INVISIBLE to the cross-layer join: this tree extracted zero joinable io (no provides, no \
             keyed consumes), so any join finding involving it is not meaningful — a framework/SDK the \
             extractor cannot see is the common cause; a Mode B adapter overlay that contributes io \
             facts (`io.provides`/`consumes`) restores visibility — an overlay carrying only imports \
             or attributes does not"
        } else {
            "visible: this tree contributed joinable io facts to the cross-layer join"
        },
    });
    // CONDITIONAL by the same convention as `query_file`'s `otherTrees`: this surface's always-present
    // norm exists for fields whose ABSENCE would be ambiguous, and an absent `walkNote` is not — it can
    // only mean "nothing under .git/ was walked". The note itself is the disclosure for the deliberate
    // no-vocabulary contract (an absent `vocabulary` yields an EMPTY skip list — see
    // `facade::config_tests`' pin): without it, VCS internals surface only as cryptic extension rows
    // like `sample`/`pack`, which nothing ties back to the config.
    if git_walked > 0 {
        view["walkNote"] = json!(format!(
            "{git_walked} file(s) under .git/ were walked into this census — the operative skip list \
             did not exclude VCS internals. A config that declares no `vocabulary.skipDirs` skips \
             nothing (a deliberate contract: absent vocabulary means an empty skip list); the starter \
             template's skipDirs excludes .git and other VCS internals"
        ));
    }
    view
}

/// Lowercased tail after the last `.` of the last path segment; whole name (lowercased) when there is
/// no dot, so `Makefile` groups as `makefile` rather than vanishing into an empty key.
fn ext_of(rel: &str) -> String {
    let base = rel.rsplit('/').next().unwrap_or(rel);
    match base.rsplit_once('.') {
        Some((_, ext)) if !ext.is_empty() => ext.to_ascii_lowercase(),
        _ => base.to_ascii_lowercase(),
    }
}

#[cfg(test)]
mod tests;
