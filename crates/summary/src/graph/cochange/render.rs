//! The mermaid-text half of the co-change domain — everything downstream of the [`Census`]. Split out
//! of `cochange.rs` on 2026-08-15 when the truncation note pushed that file past the 300-line cap;
//! `dep/render.rs` is the same split for the same reason.

use super::super::fold::Fold;
use super::Edge;

pub(super) struct Census {
    pub(super) total: usize,
    pub(super) scoped: usize,
    /// Edges remaining after the fold and BEFORE `--top` — the cap's real denominator once a fold is on.
    /// Equal to `scoped` when it is off.
    pub(super) after_fold: usize,
    /// Distinct in-scope endpoints, and how many of them this run did not analyze.
    pub(super) endpoints: usize,
    pub(super) gone: usize,
    /// Trees where git ran. The gap against `trees_total` is the "not measured" population.
    pub(super) trees_measured: usize,
    pub(super) trees_total: usize,
}

fn label(multi: bool, source: &str, rest: &str) -> String {
    let base = if multi {
        format!("{source}::{rest}")
    } else {
        rest.to_string()
    };
    base.replace('"', "'")
}

fn node_id(source: &str, path: &str) -> String {
    let mut id = String::from("cc_");
    for ch in format!("{source}::{path}").chars() {
        if ch.is_ascii_alphanumeric() {
            id.push(ch);
        } else {
            id.push('_');
        }
    }
    id
}

pub(super) fn render(
    edges: &[Edge],
    c: Census,
    multi: bool,
    scope: Option<&str>,
    top: usize,
    fold_note: &str,
    fold: Fold,
) -> String {
    let mut out = String::new();
    out.push_str("%% zzop graph --domain cochange — files that change together in git history\n");
    out.push_str(&format!(
        "%% edges: drawn {} / in-scope {} / total {} | cap --top {top}{}\n",
        edges.len(),
        c.scoped,
        c.total,
        scope.map(|s| format!(" | --scope {s}")).unwrap_or_default()
    ));
    if fold.is_on() && c.after_fold != edges.len() {
        out.push_str(&format!(
            "%% the cap applies AFTER the fold: {} of {} folded edge(s) drawn.\n",
            edges.len(),
            c.after_fold
        ));
    }
    // The two zeroes that mean different things, always named rather than left for a reader to assume.
    if c.trees_measured < c.trees_total {
        out.push_str(&format!(
            "%% NOT MEASURED in {} of {} tree(s): git was inactive or collection failed there, so their \
             absence from this picture is not evidence that nothing co-changes.\n",
            c.trees_total - c.trees_measured,
            c.trees_total
        ));
    }
    if c.trees_measured > 0 && c.total == 0 {
        out.push_str(
            "%% MEASURED AND EMPTY: git history was read and no pair cleared the filters. That is a \
             finding, not a gap.\n",
        );
    }
    out.push_str(
        "%% A SAMPLE, never a repository total: commits touching fewer than 2 or more than 25 files form \
         no pair at all, and each file keeps only its strongest partners — the tail is dropped, not \
         summed. Read an edge as \"among the strongest measured ties\", never as \"they always change \
         together\".\n",
    );
    out.push_str(
        "%% Edges are UNDIRECTED and are NOT imports. An import edge is read from source; this one is \
         read from history and says nothing about why. For imports use --domain dep.\n",
    );
    if c.gone > 0 {
        out.push_str(&format!(
            "%% {} of {} path(s) here are NOT in this run's analyzed file set: history is a different \
             population from the working tree. Each was deleted, excluded by config, or is not a source \
             file zzop parses — this picture cannot tell those apart, so it names the count instead of \
             guessing, and never drops them (a deleted module that used to pull half the repo with it is \
             exactly the boundary a reader is looking for).\n",
            c.gone, c.endpoints
        ));
    }
    out.push_str(fold_note);
    if fold.is_on() {
        out.push_str(
            "%% Pairs whose two files fold into the SAME box are dropped, not drawn as self-loops: that \
             tie is internal cohesion, and drawing it would read as a boundary where there is none.\n",
        );
    }
    out.push_str("flowchart LR\n");
    if edges.is_empty() {
        out.push_str("  none[\"no co-change pairs to draw\"]\n");
        return out;
    }
    for e in edges {
        let ia = node_id(&e.source, &e.a);
        let ib = node_id(&e.source, &e.b);
        out.push_str(&format!(
            "  {ia}[\"{}\"] --- |{}| {ib}[\"{}\"]\n",
            label(multi, &e.source, &e.a),
            e.count,
            label(multi, &e.source, &e.b)
        ));
    }
    // The VISIBLE half of every disclosure — the `%%` census above does not survive into a rendered
    // picture (dep.rs's module doc owns why both halves exist). Until 2026-08-15 this domain was the
    // ONE of five with no note path at all: a default run over a 747-pair history drew 30 edges and
    // the rendered picture said nothing. `graph --help` promises "Every cap/filter is disclosed in
    // the document (a %% census plus a visible note node)", and that sentence names all THREE
    // mechanisms — --top, --scope, --fold — so all three get a visible node here, matching the join
    // lane's dedicated SCOPED node (mermaid.rs) and dep's fold-aware unit naming.
    //
    // The truncation pointer deliberately says "raise --top", NEVER "use zzop facts": facts carries
    // `commonIr` (= output.ir) and coChange is ir's SIBLING — the facts lane has never emitted it.
    // risk.rs:175 records the last time a note here pointed at a lane that lacked the data ("a reader
    // who followed it found nothing and had no way to tell a dropped field from a lying disclosure");
    // this lane and a direct zzop-facade embedding are the only places these pairs exist.
    if let Some(p) = scope {
        if c.scoped < c.total {
            out.push_str(&format!(
                "  zzopScope[\"SCOPED to '{p}': {} of {} co-change pair(s) are in this picture — an \
                 edge counts when either endpoint is inside the scope.\"]\n",
                c.scoped, c.total
            ));
        }
    }
    if fold.is_on() {
        out.push_str(
            "  zzopFold[\"FOLDED: a node is a path-prefix box, not a file, and an edge label counts \
             the FILE-LEVEL pairs it collapsed — never a sum of commit counts.\"]\n",
        );
    }
    // Under a fold the cap applies to FOLDED edges (census line above says so), so the note counts the
    // same population the census counts — quoting raw-pair counts here would disagree with it.
    let (drawn_pop, unit) = if fold.is_on() {
        (c.after_fold, "folded edge(s)")
    } else {
        (c.scoped, "co-change pair(s)")
    };
    if edges.len() < drawn_pop {
        out.push_str(&format!(
            "  zzopNote[\"PARTIAL VIEW: {} of {} {unit} drawn ({} dropped by --top {top}). Raise \
             --top for the rest — this picture and a direct zzop-facade embedding are the only \
             surfaces that carry these pairs.\"]\n",
            edges.len(),
            drawn_pop,
            drawn_pop - edges.len()
        ));
    }
    out
}
