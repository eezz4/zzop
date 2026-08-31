//! Typing `AnalyzeRequest::adapter_overlays` — one overlay at a time, so a malformed one costs a
//! warning instead of the run.
//!
//! The field arrives as raw JSON for exactly this reason (see its own doc). Deserializing it as
//! `Vec<NormalizedEnvelope>` inside the request made one overlay's missing field the WHOLE request's
//! failure: `invalid analyze() config JSON: missing field 'loc' at line 1 column 421` — exit 1, zero
//! findings, and a coordinate into an internally re-serialized blob that appears in no file the caller
//! ever wrote. Both the field's documented contract and the adapter guide promise the opposite: an
//! invalid overlay is skipped with ONE warning naming its `parser`.
//!
//! The soft-skip machinery was never missing — `envelope::apply_adapter_overlays` re-validates every
//! overlay it is handed and degrades exactly that way. The request boundary just died first, so the
//! rest of the pipeline never got the chance. This module is that boundary, doing what the layer behind
//! it already does.

use zzop_core::NormalizedEnvelope;

/// Converts each raw overlay into a `NormalizedEnvelope`, dropping the ones that do not fit the shape
/// and pushing one warning each. The surviving overlays go on to `apply_adapter_overlays`, which
/// re-validates them against the CONTENT contract (attribution, version floor) — this pass is only
/// about whether the JSON can become the type at all.
pub(crate) fn typed_overlays(
    raw: &[serde_json::Value],
    warnings: &mut Vec<String>,
) -> Vec<NormalizedEnvelope> {
    let mut out = Vec::new();
    for (i, value) in raw.iter().enumerate() {
        match serde_json::from_value::<NormalizedEnvelope>(value.clone()) {
            Ok(envelope) => out.push(envelope),
            Err(err) => warnings.push(malformed_overlay_warning(i, value, &err.to_string())),
        }
    }
    out
}

/// Names the overlay the caller can actually find. The serde message alone carries a byte offset into a
/// re-serialization of the value, which points into nothing the caller has — so the identifying detail
/// has to be the overlay's own `parser` id, the same handle every other overlay warning uses, with the
/// array index as the fallback when the shape is broken enough that even `parser` is missing.
fn malformed_overlay_warning(index: usize, value: &serde_json::Value, detail: &str) -> String {
    let who = value
        .get("parser")
        .and_then(serde_json::Value::as_str)
        .filter(|p| !p.is_empty())
        .map(|p| format!("`adapterOverlays[{index}]` (parser `{p}`)"))
        .unwrap_or_else(|| {
            format!("`adapterOverlays[{index}]` (no `parser` id — that field is required too)")
        });
    format!(
        "{who} does not match the normalized-envelope shape and was SKIPPED: {detail}. The rest of \
         this analysis ran normally — this overlay's facts are simply absent from it, so any finding \
         that depended on them is missing rather than wrong. Byte offsets in that message index a \
         re-serialization of the value, not your file; the field name is the part to go on. To check \
         one overlay against the contract directly, validate it on its own — the `validate_envelope` \
         tool on MCP hosts, `zzop validate-envelope <file>` with the CLI binary — and read the shape \
         itself from the envelope contract: MCP resource `zzop://contract/envelope-guide` on MCP \
         hosts, `zzop contract envelope-guide` with the CLI binary."
    )
}

#[cfg(test)]
mod tests;
