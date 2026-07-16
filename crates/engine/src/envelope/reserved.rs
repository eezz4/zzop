//! Reserved engine-internal io-sentinel plumbing shared by both ingestion modes — the kind
//! predicates, the Mode B per-projection drop helper, and the shared drop-warning builder. Kept in
//! one module so Mode A (`ingest`) and Mode B (`overlay`) can't drift on WHICH kinds are reserved or
//! on how a drop is worded.

/// True iff `kind` is a reserved, engine-internal `IoProvide` sentinel that only
/// `zzop_parser_typescript::adapters::global_prefix` (native TS) may produce, and only
/// `compose::apply_and_strip_global_prefix` (the native `analyze::assemble` pipeline) may consume+strip —
/// see that pair's docs. A producer feeding this engine any other way (an envelope's `FileProjection`,
/// Mode A or Mode B) must never emit it: envelope/overlay ingestion never runs that consuming seam, so a
/// leaked sentinel would either surface raw in output/rules (Mode A) or get re-applied against the WHOLE
/// native tree by that seam once merged (Mode B) — an external overlay author re-prefixing every native
/// route by accident. Both `analyze_envelope` (Mode A) and `apply_adapter_overlays` (Mode B)
/// call this — kept as one predicate so the two modes can't drift on which kinds are reserved.
///
/// Bound to the parser's exported const (not a local literal) so a rename on the emit side cannot
/// silently desynchronize this check — a leaked sentinel would reach output.
pub(super) fn is_reserved_provide_kind(kind: &str) -> bool {
    kind == zzop_parser_typescript::NEST_GLOBAL_PREFIX_KIND
}

/// True iff `kind` is the `IoConsume` counterpart of [`is_reserved_provide_kind`] — the client-base-prefix
/// sentinel only `zzop_parser_typescript::adapters::client_base` may produce and only
/// `compose::apply_client_base_prefixes` may consume+strip. Same producer-forbidden rationale.
pub(super) fn is_reserved_consume_kind(kind: &str) -> bool {
    kind == zzop_parser_typescript::CLIENT_BASE_PREFIX_KIND
}

/// Builds the one aggregate "reserved sentinel(s) dropped" warning shared by both modes — `Some` iff
/// `dropped > 0`. `subject_kind` is the noun phrase (`"envelope"` for Mode A, `"adapter overlay"` for
/// Mode B) and `subject_id` is that mode's own identifier (`NormalizedEnvelope::parser` in both cases,
/// since Mode B's overlays ARE `NormalizedEnvelope`s too — an envelope's `source` is not used here since
/// `parser` is what a producer actually recognizes as "mine"). Centralizing the count->message step here
/// (rather than duplicating the singular/plural + kind-list text in each call site) is what keeps the two
/// modes' wording from drifting apart the way `is_reserved_provide_kind`/`is_reserved_consume_kind` keep
/// them from drifting on WHICH kinds are reserved.
pub(super) fn reserved_drop_warning(
    subject_kind: &str,
    subject_id: &str,
    dropped: usize,
) -> Option<String> {
    if dropped == 0 {
        return None;
    }
    let entries = if dropped == 1 { "entry" } else { "entries" };
    // Built from the two producers' own exported consts (not hardcoded literals) so this text can never
    // drift from the real kinds `is_reserved_provide_kind`/`is_reserved_consume_kind` check — the
    // rendered string is unchanged from before (both consts equal the literals this format! replaces).
    let nest_global_prefix_kind = zzop_parser_typescript::NEST_GLOBAL_PREFIX_KIND;
    let client_base_prefix_kind = zzop_parser_typescript::CLIENT_BASE_PREFIX_KIND;
    Some(format!(
        "{subject_kind} '{subject_id}': dropped {dropped} reserved engine-internal io {entries} \
         (kinds `{nest_global_prefix_kind}`/`{client_base_prefix_kind}` are producer-forbidden)"
    ))
}

/// Returns a clone of `projection` with every reserved engine-internal `IoProvide`/`IoConsume` entry
/// dropped from its `io` (see [`is_reserved_provide_kind`]/[`is_reserved_consume_kind`]), plus how many
/// entries were dropped — the Mode B (`apply_adapter_overlays`) counterpart of Mode A's own ingestion-time
/// filter in `analyze_envelope`. Every other field is untouched.
pub(super) fn drop_reserved_io(
    projection: &zzop_core::FileProjection,
) -> (zzop_core::FileProjection, usize) {
    let mut cleaned = projection.clone();
    let before = cleaned.io.provides.len() + cleaned.io.consumes.len();
    cleaned
        .io
        .provides
        .retain(|p| !is_reserved_provide_kind(&p.kind));
    cleaned
        .io
        .consumes
        .retain(|c| !is_reserved_consume_kind(&c.kind));
    let after = cleaned.io.provides.len() + cleaned.io.consumes.len();
    (cleaned, before - after)
}
