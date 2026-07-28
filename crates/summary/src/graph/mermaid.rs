//! The mermaid emitter: the model built in the parent module -> flowchart text. Pure string building,
//! no analysis decisions of its own — every judgment (what is in scope, what was capped, what could not
//! be labelled) arrives already made, so this file can only under- or over-STATE, never re-derive.

use super::model::{BucketCount, Graph, NodeKey, PROVIDE};

/// The seven role classes plus the disclosure class, each declared once. Colours are a redundant
/// channel only: every node's role is also spelled in its LABEL, so a viewer that drops `classDef`
/// (or a colour-blind reader) loses styling, never meaning.
const CLASS_DEFS: [(&str, &str); 8] = [
    ("linked", "fill:#e6f4ea,stroke:#1e7d32,color:#111"),
    ("candidate", "fill:#f1f8e9,stroke:#558b2f,color:#111"),
    ("unconsumed", "fill:#fff4e5,stroke:#b26a00,color:#111"),
    ("unprovided", "fill:#fdecea,stroke:#c62828,color:#111"),
    ("ambiguous", "fill:#f3e5f5,stroke:#7b1fa2,color:#111"),
    ("unresolved", "fill:#eceff1,stroke:#546e7a,color:#111"),
    ("external", "fill:#e8eaf6,stroke:#3949ab,color:#111"),
    (
        "note",
        "fill:#ffffff,stroke:#111,color:#111,stroke-dasharray:4 3",
    ),
];

/// Renders the whole document. Order is fixed and total: header comments, then one subgraph per source
/// (sorted), then the edges (sorted), then the disclosure nodes, then the class definitions.
pub(super) fn render(g: &Graph, counts: &[BucketCount], scope: Option<&str>, top: usize) -> String {
    // The diagram declaration leads the document and the `%%` census follows it INSIDE the body.
    // Comments are unambiguously legal inside a flowchart; whether a renderer tolerates them BEFORE
    // the declaration (which is what decides the diagram type) is renderer-dependent, and a header
    // that risks making the whole document unrenderable would defeat its own purpose.
    let mut out = String::from("flowchart LR\n");
    out.push_str(&header(counts, scope, top));

    // Node ids are positional over the already-sorted node map, which is what keeps arbitrary key text
    // (spaces, slashes, braces, quotes) out of mermaid IDENTIFIER position entirely — it only ever
    // reaches a quoted label, where `escape` handles it.
    let ids: Vec<(&NodeKey, String)> = g
        .nodes
        .keys()
        .enumerate()
        .map(|(i, k)| (k, format!("n{i}")))
        .collect();
    let id_of = |key: &NodeKey| {
        ids.iter()
            .find(|(k, _)| *k == key)
            .map(|(_, id)| id.as_str())
    };

    for (i, (source, zero_contribution)) in g.sources.iter().enumerate() {
        out.push_str(&format!("  subgraph s{i}[\"{}\"]\n", escape(source)));
        let mut drawn = 0;
        for (key, id) in &ids {
            if &key.0 != source {
                continue;
            }
            let role = g.nodes[*key];
            let label = escape(&format!("{role} · {} {}", key.2, key.3));
            // Rectangle = provide (a served thing), stadium = consume (a call site) — a second
            // redundant channel, like the colours, never the only one.
            let shape = if key.1 == PROVIDE {
                format!("[\"{label}\"]")
            } else {
                format!("([\"{label}\"])")
            };
            out.push_str(&format!("    {id}{shape}:::{role}\n"));
            drawn += 1;
        }
        if drawn == 0 {
            // An absent/empty subgraph would read as "this tree is fine". Both zero states say which
            // zero they are.
            let why = if *zero_contribution {
                "extracted no joinable io — invisible to the join (blindness, not an empty contract)"
            } else {
                "no rows in this view (see the scope/top disclosure above)"
            };
            out.push_str(&format!("    s{i}note[\"{}\"]:::note\n", escape(why)));
        }
        out.push_str("  end\n");
    }

    for edge in &g.edges {
        let (Some(from), Some(to)) = (id_of(&edge.from), id_of(&edge.to)) else {
            continue;
        };
        let arrow = match (edge.dotted, &edge.label) {
            (false, None) => " --> ".to_string(),
            (true, None) => " -.-> ".to_string(),
            (false, Some(l)) => format!(" -- \"{}\" --> ", escape(l)),
            (true, Some(l)) => format!(" -. \"{}\" .-> ", escape(l)),
        };
        out.push_str(&format!("  {from}{arrow}{to}\n"));
    }

    for (i, note) in disclosure_notes(counts, scope, top).iter().enumerate() {
        out.push_str(&format!("  disclosure{i}[\"{}\"]:::note\n", escape(note)));
    }
    for (name, style) in CLASS_DEFS {
        out.push_str(&format!("  classDef {name} {style}\n"));
    }
    out
}

/// The `%%` header. Mermaid comments do not render, so this is the MACHINE-readable half of the
/// disclosure (a full per-bucket census, always, capped or not); the half that survives into the
/// picture is [`disclosure_notes`].
fn header(counts: &[BucketCount], scope: Option<&str>, top: usize) -> String {
    let mut out = String::from(
        "%% zzop graph — the cross-layer join as a mermaid flowchart (paste into any mermaid viewer).\n",
    );
    out.push_str(&format!("%% tool: {}\n", zzop_facade::version_string()));
    out.push_str(&format!(
        "%% scope: {}\n",
        scope.unwrap_or("(none — every source)")
    ));
    out.push_str(&format!("%% top: {top} drawn relations per bucket\n"));
    out.push_str("%% relations drawn/inScope/total (and the call sites they aggregate):\n");
    for c in counts {
        out.push_str(&format!(
            "%%   {}: {}/{}/{} from {} site(s){}\n",
            c.bucket,
            c.shown,
            c.in_scope,
            c.total,
            c.rows,
            if c.unlabelable > 0 {
                format!(
                    " (+{} with neither key nor raw — not labelable, never guessed)",
                    c.unlabelable
                )
            } else {
                String::new()
            }
        ));
    }
    out.push_str(
        "%% a node is (source, side, kind, key): CALL SITES ARE AGGREGATED and no file/line appears \
         here — use `zzop facts` or `zzop cross` for per-site detail.\n",
    );
    out.push_str(
        "%% NOT rendered by this surface: crossLayerFindings (the drift/near-miss VERDICTS — findings \
         about the join, not members of it), hostRekeyCounts, warnings/configWarnings/disclosure.\n",
    );
    out
}

/// The disclosure that survives RENDERING: one visible node per active mechanism. A `%%` comment is
/// invisible in a drawn diagram, and a picture that silently omits rows is the failure this project
/// forbids — so a truncated or filtered graph says so ON the canvas.
fn disclosure_notes(counts: &[BucketCount], scope: Option<&str>, top: usize) -> Vec<String> {
    let mut notes = Vec::new();
    let truncated: Vec<String> = counts
        .iter()
        .filter(|c| c.in_scope > c.shown)
        .map(|c| format!("{} {}/{}", c.bucket, c.shown, c.in_scope))
        .collect();
    if !truncated.is_empty() {
        notes.push(format!(
            "TRUNCATED — {} (drawn/inScope relations). Raise --top (now {top}) or narrow --scope; `zzop facts` is uncapped.",
            truncated.join(", ")
        ));
    }
    if let Some(prefix) = scope {
        let hidden: usize = counts.iter().map(|c| c.total - c.in_scope).sum();
        notes.push(format!(
            "SCOPED to '{prefix}' — {hidden} relation(s) outside the scope are not drawn."
        ));
    }
    notes
}

/// Label sanitation, and the only text transformation in this file. Whitespace runs collapse to a
/// single space (a raw call-site expression can be multi-line, and a newline inside a label would
/// break the document, not just the picture); the four characters mermaid reads structurally inside a
/// quoted label become its own entity codes. Nothing is truncated — a long label is ugly, but an
/// elided one would be a second truncation channel to disclose.
fn escape(s: &str) -> String {
    let collapsed = s.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed
        .replace('#', "#35;")
        .replace('"', "#quot;")
        .replace('<', "#lt;")
        .replace('>', "#gt;")
}
