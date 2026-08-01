//! `queryFile` — a DEFINITIVE answer to "what does zzop know about THIS FILE?", computed by pure
//! post-processing over an already-produced `analyzeTrees` output. Same contract as [`super::query`]:
//! no re-analysis, no cache interaction, one core shared by every host.
//!
//! # Why a second target axis, and why this one (D16)
//! The instruction that produced this was *"a small reasoning model needs a focused target version"*,
//! immediately corrected to **"it is TARGETING information, not saying less"**. That correction is the
//! whole design. This surface drops nothing: the caller NAMES a target and gets everything about it, so
//! the question "how do we disclose what was truncated" never arises — the honesty problem a
//! smaller-output mode would have created is designed out rather than managed.
//!
//! Before this, `check_endpoint` was the only targeted surface in the product and its target was an io
//! KEY. `analyze_repo`/`cross_repo` have no target at all: hand them a tree and they answer about the
//! whole tree. A model with little room therefore received "everything" and had to choose — and a model
//! that cannot choose treats the first screen as the conclusion.
//!
//! **The axis is a FILE PATH** — chosen over symbol / rule id / bucket / tree because it is the one thing
//! an agent always already has: it just opened, wrote, or was asked about a file. The others are either
//! already served (an io key by `check_endpoint`; a rule id by the `--rule` findings filter) or derivable
//! from this one.
//!
//! # The verdict answers "was this file analyzed", not "is this file healthy"
//! This is the part worth being deliberate about. An empty findings list has two completely different
//! meanings — *clean* and *never looked at* — and every other surface in this repo works hard to keep
//! them apart. A targeted file query is where they collide most sharply, because a caller asking about
//! one file will read silence as an all-clear. So the verdict is about ANALYSIS STATE and is answered
//! first; findings ride along underneath it.
//!
//! | verdict         | meaning                                                                     |
//! |-----------------|-----------------------------------------------------------------------------|
//! | `analyzed`      | a structural projection exists (symbols and/or dep-graph membership)         |
//! | `lexical-only`  | walked and line-scanned, but no structural projection — no parser for it     |
//! | `degraded`      | the parser bailed (syntax error, or over `sizeCap`); projection is empty     |
//! | `not-found`     | this run never walked that path                                              |
//!
//! Each token's one-sentence meaning ships in the reply as `verdictMeaning`, the same self-describing
//! discipline `query_io`'s vocabulary uses — no host's help text is a second owner of what a token means.
//!
//! `dependencies` gets the same treatment (`dependenciesMeaning`, 2026-07-31) because it is the other
//! field here whose SILENCE is ambiguous: the dep graph carries resolved in-tree edges only, so an empty
//! `imports` says "no in-tree import of this file resolved", never "this file imports nothing".
//!
//! `analyzed` deliberately does NOT distinguish native parsing from a Mode-B adapter overlay. The
//! question this token answers is "does a structural projection exist for this file", and for that
//! purpose an overlay IS its parser — splitting the token would make callers branch on a distinction
//! that changes nothing about what they can then ask.
//!
//! # Uncapped on purpose
//! `query_io` caps each match bucket because a pattern can match hundreds of keys. A single file's
//! findings, io facts and edges are bounded by the file itself, and capping them would reintroduce
//! exactly the truncation-disclosure problem this surface exists to avoid. If some pathological file
//! produces a huge list, the honest answer is still the whole list.

use serde_json::{json, Map, Value};

/// The sealed verdict vocabulary — wire contract, do not extend without a contract bump.
pub const FILE_VERDICTS: &[&str] = &["analyzed", "lexical-only", "degraded", "not-found"];

/// One sentence per token, shipped in the reply so the vocabulary is self-describing.
fn verdict_meaning(verdict: &str) -> &'static str {
    match verdict {
        "analyzed" => {
            "zzop built a structural projection for this file (symbols and/or dependency-graph \
             membership), so every rule that needs structure was able to run on it. An empty findings \
             list here means clean."
        }
        "lexical-only" => {
            "zzop walked this file and ran text-based (line-scan) rules over it, but built no \
             structural projection — no parser in this build claims its extension. An empty findings \
             list here does NOT mean clean: everything that needs symbols, imports or io facts was \
             never evaluated. Bring an adapter overlay, or declare a parser for the extension under \
             `parsers.globOverrides`."
        }
        "degraded" => {
            "zzop tried to parse this file and could not — a syntax error its parser does not tolerate, \
             or a file over `sizeCap`. Text-based rules still ran; everything structural did not. An \
             empty findings list here does NOT mean clean."
        }
        "not-found" => {
            "This run never walked that path. It may be excluded by config (`exclude`, \
             `vocabulary.skipDirs`), outside every declared tree root, or simply misspelled — the \
             `suggestions` field lists the nearest walked paths."
        }
        _ => "unknown verdict token",
    }
}

/// Cap on `suggestions` for a `not-found` target — the one capped list here, because it is a ranking
/// over every walked path rather than a fact about the target.
const MAX_SUGGESTIONS: usize = 10;

/// Answers `{"path": "<tree-relative or absolute-ish path>"}` against an `analyzeTrees` output.
///
/// The target is matched against each tree's own relative paths. A caller who knows which tree it means
/// may pass `sourceId` to disambiguate; without it, every tree is searched and the reply names the tree
/// the match came from. A path present in two trees yields the first by tree order, with `otherTrees`
/// naming the rest — never a silent pick.
pub fn query_file_json(analysis_json: &str, query_json: &str) -> Result<String, String> {
    let analysis: Value =
        serde_json::from_str(analysis_json).map_err(|e| format!("analysis JSON: {e}"))?;
    let query: Value = serde_json::from_str(query_json).map_err(|e| format!("query JSON: {e}"))?;
    let target = query
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| "query needs a `path` string".to_string())?;
    let want_tree = query.get("sourceId").and_then(Value::as_str);

    let trees = analysis
        .get("trees")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            "this analysis has no `trees` — the file query runs over an analyzeTrees output \
             (a single-tree `analyze` output has no tree identity to report)"
                .to_string()
        })?;

    let normalized = normalize(target);
    let mut hits: Vec<(&Value, String)> = Vec::new();
    for tree in trees {
        if let Some(want) = want_tree {
            if tree.get("sourceId").and_then(Value::as_str) != Some(want) {
                continue;
            }
        }
        if let Some(rel) = match_rel(tree, &normalized) {
            hits.push((tree, rel));
        }
    }

    let Some((tree, rel)) = hits.first() else {
        return Ok(pretty(not_found(target, trees, &normalized)));
    };

    let mut out = Map::new();
    out.insert("target".to_string(), json!(rel));
    out.insert(
        "sourceId".to_string(),
        tree.get("sourceId").cloned().unwrap_or(Value::Null),
    );
    let verdict = verdict_for(tree, rel);
    out.insert("verdict".to_string(), json!(verdict));
    out.insert(
        "verdictMeaning".to_string(),
        json!(verdict_meaning(verdict)),
    );
    if hits.len() > 1 {
        // Never a silent pick: the caller is told the same relative path exists in other trees, so a
        // wrong-tree answer is visible rather than plausible.
        out.insert(
            "otherTrees".to_string(),
            json!(hits[1..]
                .iter()
                .map(|(t, _)| t.get("sourceId").cloned().unwrap_or(Value::Null))
                .collect::<Vec<_>>()),
        );
    }
    out.insert("loc".to_string(), json!(loc_of(tree, rel)));
    out.insert("symbols".to_string(), symbols_of(tree, rel));
    out.insert("io".to_string(), io_of(tree, rel));
    out.insert("dependencies".to_string(), deps_of(tree, rel));
    // The same self-describing discipline `verdictMeaning` established, applied to the one other field
    // here whose EMPTINESS is ambiguous: an empty `imports` reads as "this file imports nothing" when it
    // actually says "no in-tree import of it resolved". See `facets::dependencies_meaning`.
    out.insert(
        "dependenciesMeaning".to_string(),
        json!(dependencies_meaning()),
    );
    out.insert("findings".to_string(), findings_of(&analysis, tree, rel));
    Ok(pretty(Value::Object(out)))
}

fn pretty(v: Value) -> String {
    serde_json::to_string_pretty(&v).unwrap_or_else(|_| v.to_string())
}

/// Path normalization the whole module shares: backslashes to slashes, no leading `./`. Deliberately NOT
/// canonicalization — this core never touches the filesystem, so it cannot resolve symlinks or `..`, and
/// pretending otherwise would make the match depend on a disk the analysis no longer has.
fn normalize(p: &str) -> String {
    p.replace('\\', "/").trim_start_matches("./").to_string()
}

/// The tree-relative path this target names, if the tree walked it. Accepts an exact relative path, or
/// an absolute-ish path whose tail matches one — an agent usually has the absolute path in hand.
fn match_rel(tree: &Value, normalized: &str) -> Option<String> {
    let loc = tree.pointer("/output/ir/loc").and_then(Value::as_object)?;
    if loc.contains_key(normalized) {
        return Some(normalized.to_string());
    }
    // Suffix match, longest first so `src/api/users.ts` never loses to `users.ts`.
    let mut candidates: Vec<&String> = loc
        .keys()
        .filter(|k| {
            normalized.ends_with(&format!("/{k}")) || k.ends_with(&format!("/{normalized}"))
        })
        .collect();
    candidates.sort_by_key(|k| std::cmp::Reverse(k.len()));
    candidates.first().map(|k| (*k).clone())
}

fn verdict_for(tree: &Value, rel: &str) -> &'static str {
    let degraded = tree
        .pointer("/output/degraded")
        .and_then(Value::as_array)
        .is_some_and(|a| a.iter().any(|d| d.as_str() == Some(rel)));
    if degraded {
        return "degraded";
    }
    let has_symbols = tree
        .pointer("/output/ir/symbols")
        .and_then(Value::as_array)
        .is_some_and(|a| {
            a.iter()
                .any(|s| s.get("file").and_then(Value::as_str) == Some(rel))
        });
    let in_dep = tree
        .pointer("/output/ir/dep")
        .and_then(Value::as_object)
        .is_some_and(|d| d.contains_key(rel));
    if has_symbols || in_dep {
        "analyzed"
    } else {
        "lexical-only"
    }
}

/// The `not-found` reply, with the nearest walked paths so a typo is one step from fixed rather than a
/// dead end. Ranked by shared trailing segments, then by length — a deterministic ordering, never a fuzzy
/// score whose ties would depend on hash order.
fn not_found(target: &str, trees: &[Value], normalized: &str) -> Value {
    let base = normalized.rsplit('/').next().unwrap_or(normalized);
    let mut scored: Vec<(usize, usize, String)> = Vec::new();
    for tree in trees {
        let Some(loc) = tree.pointer("/output/ir/loc").and_then(Value::as_object) else {
            continue;
        };
        for k in loc.keys() {
            let k_base = k.rsplit('/').next().unwrap_or(k);
            let score = if k_base == base {
                3
            } else if k_base.contains(base) || base.contains(k_base) {
                2
            } else if k.contains(normalized) {
                1
            } else {
                0
            };
            if score > 0 {
                scored.push((score, k.len(), k.clone()));
            }
        }
    }
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    let total = scored.len();
    let suggestions: Vec<String> = scored
        .into_iter()
        .take(MAX_SUGGESTIONS)
        .map(|(_, _, k)| k)
        .collect();
    let mut out = json!({
        "target": target,
        "verdict": "not-found",
        "verdictMeaning": verdict_meaning("not-found"),
        "suggestions": suggestions,
    });
    if total > MAX_SUGGESTIONS {
        out["suggestionsTruncated"] = json!(total - MAX_SUGGESTIONS);
    }
    out
}

mod facets;
use facets::{dependencies_meaning, deps_of, findings_of, io_of, loc_of, symbols_of};

#[cfg(test)]
mod tests;
