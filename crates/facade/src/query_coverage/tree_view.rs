//! One tree's coverage row — the per-tree half of the reply `super` assembles.
//!
//! Split from `super` when that file crossed the 300-line cap, along the seam it always had: the
//! parent composes the ROOT (the tree array, the build-constant legends and capability tables), and
//! this file answers "what does one tree look like". The two never shared a helper — the three
//! functions here are used by nothing else — which is what made the cut obvious rather than chosen.

use serde_json::{json, Value};

use super::native_roster::native_analyses_of;
use super::{blind_spots, ext_of, io_channels, join_visibility, unread};
pub(super) fn tree_view(tree: &Value, sightlines: &[zzop_core::RuleSightline]) -> Value {
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
    // The THIRD projection channel, and the one this set omitted until 2026-08-20: io facts. A parser
    // frontend that projects neither symbols nor dep edges is not hypothetical — `zzop-parser-sql` is
    // exactly that by design (`dispatch.rs`: "`db-table` io PROVIDEs only ... no symbols/imports
    // project for `.sql`"), and `Prisma` shares the shape. The cost was measured on macrozheng/mall:
    // `{"ext":"sql","files":1,"structural":0,"lexicalOnly":1}` next to a census reading
    // `parserDispatched: 525` (524 java + that file) and all 76 of the run's `db-table` provides
    // coming out of it, while `zzop version --verbose` advertised the frontend that read it. A reader
    // auditing coverage concluded the file was never parsed; it was parsed, and every fact it yielded
    // is in this same reply. `lexicalOnly`'s own legend promises "no parser in this build claims the
    // extension", which was false for that row — the honest fix is to count the channel, not to
    // reword the promise.
    extend_with_io_files(&mut structural, tree.pointer("/output/ir/io"));

    // Fetched before the extension table below, which reads its `declaredImportsByExt` half (F4);
    // forwarded verbatim as the `census` field further down, exactly as before.
    let census = tree
        .pointer("/output/coverage")
        .cloned()
        .unwrap_or(Value::Null);
    // F4 declared-side lookup: an extension key ABSENT here renders as a `null` cell — never measured
    // (channel-less parser, or a Mode A envelope run), not 0. See the `declaredImports` legend.
    let declared = census
        .get("declaredImportsByExt")
        .and_then(Value::as_object);

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
                    "inDepGraph": in_dep,
                    "declaredImports": declared.and_then(|m| m.get(ext)).cloned()
                                               .unwrap_or(Value::Null) })
        })
        .collect();
    // The extensions with 1+ structural file — the measured half of the `blindSpots` cross, and
    // (with their file counts) of `ioChannels`' `zeroExtraction` cross.
    let structural_by_ext: std::collections::BTreeMap<String, usize> = by_ext
        .iter()
        .filter(|(_, counts)| counts.1 > 0)
        .map(|(ext, counts)| (ext.clone(), counts.1))
        .collect();
    let structural_exts: std::collections::BTreeSet<String> =
        structural_by_ext.keys().cloned().collect();

    // Bound before the view so `blindSpotBasis` names the exclusion off the very list it emits.
    let unread = unread::extensions(by_ext.iter().map(|(e, c)| (e.as_str(), c.0, c.1)));

    let join_zero = census
        .get("joinContributionZero")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut view = json!({
        "sourceId": tree.get("sourceId").cloned().unwrap_or(Value::Null),
        "extensions": extensions,
        "blindSpots": blind_spots::blind_spots(sightlines, &structural_exts),
        // What the cross was computed from AND what it excluded — see `blind_spots::basis`.
        "blindSpotBasis": blind_spots::basis(sightlines.len(), structural_exts.len(), &unread),
        // The excluded population itself: principal filetypes no structural parser read.
        "unreadExtensions": unread,
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
        "joinVisibility": join_visibility::join_visibility(&census, join_zero),
        // The per-CHANNEL view `joinVisibility` above and the census's `joinContributionZero` both
        // structurally cannot give: those two ask ONE question of the whole io contribution, so a
        // full channel vouches for an empty one (measured on gogs — 12 db-table provides, 0 http
        // routes, "contributed joinable io"). It also carries the zero-extraction cross that names
        // an empty channel by LANGUAGE instead of by recognized framework name. See `io_channels`.
        "ioChannels": io_channels::io_channels(tree, &structural_by_ext),
    });
    // CONDITIONAL by the same convention as `query_file`'s `otherTrees`: this surface's always-present
    // norm exists for fields whose ABSENCE would be ambiguous, and an absent `walkNote` is not — it can
    // only mean "nothing under .git/ was walked". The note itself is the disclosure for the deliberate
    // no-vocabulary contract (an absent `vocabulary` yields an EMPTY skip list — see
    // `facade::config_tests`' pin): without it, VCS internals surface only as cryptic extension rows
    // like `sample`/`pack`, which nothing ties back to the config.
    // The analyses this BUILD ships off, forwarded from the tree that measured them. CONDITIONAL for
    // the reason above and one more: an engine older than the field publishes no roster, and writing
    // `null` here would state that this build ships nothing off, which is the opposite of the truth.
    if let Some(native) = native_analyses_of(tree) {
        view["nativeAnalyses"] = native;
    }
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

/// Adds every file named by an `ir.io` provide or consume to `set`. Split out because BOTH extension
/// bucketings — this module's and `zzop_summary`'s `coverageGaps`, which its own doc pins as this one's
/// arm-for-arm mirror — must fold the same three channels in, and a channel added to one and not the
/// other is how the two surfaces come to disagree about the same file.
///
/// A `null`/absent `io` block (an older shape, or an envelope run that measured none) contributes
/// nothing rather than erroring: absence of the channel is not evidence about any file.
fn extend_with_io_files<'a>(set: &mut std::collections::HashSet<&'a str>, io: Option<&'a Value>) {
    let Some(io) = io.and_then(Value::as_object) else {
        return;
    };
    for side in ["provides", "consumes"] {
        let Some(rows) = io.get(side).and_then(Value::as_array) else {
            continue;
        };
        set.extend(
            rows.iter()
                .filter_map(|r| r.get("file").and_then(Value::as_str)),
        );
    }
}
