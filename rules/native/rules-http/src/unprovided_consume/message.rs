//! Both `unprovided-consume` finding shapes (individual + foreign-fold aggregate) — split out of the
//! parent module to keep that file under the 300-line source cap. Message contract lives here in one
//! place: problem, fix, every veto that could have suppressed this finding, and how to turn the rule off.

/// The individual finding for one unmatched consume — both non-folded legs (parent module doc:
/// "overlapping", and "foreign" below the fold threshold).
///
/// `key` is always the JOIN key (post host re-key), matching the linker's own bucket invariant that
/// nothing downstream of the re-key ever sees a scheme-carrying key. `raw` carries the original
/// absolute-URL spelling when a declared-host re-key rewrote it, so the message can point at the line the
/// author actually wrote.
pub(super) fn individual_finding(
    key: &str,
    raw: Option<&str>,
    file: &str,
    line: u32,
) -> zzop_core::Finding {
    // Paste-ready `routes` stub (single-tree, so the serving tree is this one — no cross-tree ambiguity).
    let injection_stub = format!("routes: [{{ \"key\": \"{key}\", \"role\": \"provide\" }}]");
    let rekey_clause = match raw {
        Some(raw) => format!(
            " (the call is written as `{raw}`; its host is declared in this analysis's `hosts`, so it was \
             re-keyed to the internal path above before matching, exactly as the cross-layer linker does)"
        ),
        None => String::new(),
    };
    let mut data = serde_json::json!({ "key": key, "injectionStub": injection_stub });
    if let Some(raw) = raw {
        data["rawKey"] = serde_json::json!(raw);
    }
    zzop_core::Finding {
        rule_id: "unprovided-consume".to_string(),
        severity: zzop_core::Severity::Info,
        file: file.to_string(),
        line,
        message: format!(
            "This call consumes `{key}`{rekey_clause} but no HTTP route anywhere in this analysis provides \
             that key — likely a typo'd path, a renamed/removed backend route, or a route defined in a file \
             this analysis didn't parse. Verify the route still exists at that path and method; if it does, inject it with `{injection_stub}`. \
             Veto: a key path ending in a static-asset extension (.js/.mjs/.cjs, .map, .css, .txt, .svg, \
             .png, .jpg/.jpeg, .gif, .ico, .bmp, .avif, .webp, .woff/.woff2/.ttf/.otf/.eot) is never \
             flagged; .json/.xml is vetoed by default UNLESS the path carries an API-ish segment (/api/, \
             /graphql/, /rpc/, or a version segment like /v1/), so `GET /i18n/ko.json` is vetoed while \
             `GET /api/users.json` is real API consumption (Rails-style format-suffixed route) and stays \
             flaggable — tradeoff: a Rails-style .json/.xml API route outside any /api-ish segment is \
             missed too. An absolute-URL consume (any key containing `://`, localhost/127.0.0.1 dev \
             self-references included) is never flagged either: that is third-party egress by the \
             cross-layer linker's own contract, which routes such a key to `externalConsumes` rather than \
             `unprovidedConsumes` — unless its host is one this analysis declares in `hosts`, which re-keys \
             it to an internal path and puts it back in scope, same as multi-tree. A key whose path is \
             ALL `{{}}` placeholders (`GET /{{}}`) is vetoed too: it names no route, so its failure to match \
             is an extraction gap, not a missing contract — the multi-tree join reports such a key under \
             `unresolvedConsumes` and counts it in `cross-layer/unresolved-consume-ratio`. Counting note: several \
             unmatched consumes OUTSIDE this source's own path families collapse into ONE aggregate \
             finding at 3+, and the vetoes above are applied FIRST — so vetoing one sibling can drop a \
             group from 3 to 2 and turn that single aggregate back into 2 individual findings like this \
             one. Fewer keys are reported, but the finding COUNT can rise and an individual finding can \
             appear at a line the aggregate never anchored to. Note: this only fires because this same \
             source ALSO provides at least one HTTP route itself — a source with zero HTTP provides is \
             assumed to be consuming a remote backend outside this analysis's scope and is never flagged by \
             this rule (that veto avoids a systematic false-positive class for pure front-end sources). If \
             you're analyzing a split FE/BE repo pair, prefer the multi-source `analyze_trees` cross-layer \
             join (`MultiAnalyzeOutput::cross_layer.unprovided_consumes`), which matches consumes against \
             every source's provides, not just this one. This finding starts at Info severity: provide \
             extraction is evidence-gated, so route shapes it cannot prove (dynamic or `startsWith` path \
             matching, const-indirected path literals, raw-Worker dispatch outside the `pathname_dispatch` \
             adapter's Request-evidence gate) remain a structural false-positive source. {} if intentional \
             (this rule has no inline suppression marker).",
            zzop_core::disable_hint("unprovided-consume")
        ),
        evidence_paths: Vec::new(),
        data: Some(data),
    }
}

/// Inputs for [`aggregate_finding`] — the ONE finding that replaces N foreign individual ones (parent
/// module doc "Foreign-vs-overlapping fold").
pub(super) struct Aggregate<'a> {
    /// Folded consume count BEFORE key dedup — what "this replaces N findings" honestly means.
    pub call_count: usize,
    /// Deduped, sorted join keys.
    pub routes: &'a [&'a str],
    /// Absolute spellings of any folded entry a declared-host re-key rewrote; empty when none was.
    pub raws: &'a [&'a str],
    pub path_space_clause: &'a str,
    pub provide_first_segments: &'a std::collections::BTreeSet<&'a str>,
    pub file: &'a str,
    pub line: u32,
}

/// The aggregate finding. Replace-not-silently-suppress: every folded key is enumerated in both
/// `data.routes` and the message body, so the fold loses no information.
pub(super) fn aggregate_finding(a: Aggregate) -> zzop_core::Finding {
    let n = a.call_count;
    let path_space_clause = a.path_space_clause;
    // A folded entry is listed under its JOIN key, so a re-keyed one shows an internal path the author
    // cannot grep for — name the absolute spellings too, and only when a re-key actually happened.
    let raw_clause = if a.raws.is_empty() {
        String::new()
    } else {
        format!(
            " Keys written as absolute URLs to a host this analysis declares in `hosts` are listed above \
             under the internal path they were re-keyed to; as written in the source they are: {}.",
            a.raws.join(", ")
        )
    };
    let message = format!(
        "{n} calls in this tree consume HTTP keys that no route in this analysis provides, and none \
         of them fall under this tree's own provided path space ({path_space_clause}) — this tree \
         looks like a partial provider (e.g. a monorepo where only one app's routes are visible), so \
         these calls are most likely served by something outside this analysis rather than being {n} \
         independent broken routes. Affected keys: {}. This replaces {n} individual \
         `unprovided-consume` findings. If these should have local providers, each key above is a real \
         gap.{raw_clause} {} if intentional (this rule has no inline suppression marker).",
        a.routes.join(", "),
        zzop_core::disable_hint("unprovided-consume"),
    );
    let mut data = serde_json::json!({
        "callCount": n,
        "routes": a.routes,
        "provideFirstSegments": a.provide_first_segments.iter().collect::<Vec<_>>(),
    });
    if !a.raws.is_empty() {
        data["rawKeys"] = serde_json::json!(a.raws);
    }
    zzop_core::Finding {
        rule_id: "unprovided-consume".to_string(),
        severity: zzop_core::Severity::Info,
        file: a.file.to_string(),
        line: a.line,
        message,
        evidence_paths: Vec::new(),
        data: Some(data),
    }
}
