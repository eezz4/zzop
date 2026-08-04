//! The VALIDITY judgment on a `NormalizedEnvelope` — `validate_envelope` and everything it needs.
//!
//! Split out of `normalized.rs` when that file crossed the 300-line ceiling `check-max-file-lines`
//! enforces. The seam is not arbitrary: the parent module declares the contract SHAPE (the serde types
//! a producer writes against), this one decides whether a given envelope satisfies it. The advisory
//! axis is a third file again (`hints.rs`) — rejection and advice must not be able to leak into each
//! other, which is the same reason `EnvelopeVerdict` keeps them as separate fields.

use super::hints::envelope_hints;
use super::{
    parse_contract_version, NormalizedEnvelope, MIN_VERSION_FOR_CALLS, MIN_VERSION_FOR_OVERRIDES,
    NORMALIZED_AST_CONTRACT_VERSION, NORMALIZED_AST_FORMAT, SUPPORTED_NORMALIZED_AST_VERSION,
};
/// Validates `json` against the Normalized AST contract (`docs/NORMALIZED_AST.md`) beyond what plain
/// `serde_json` deserialization alone checks — a wrong `format` string or an out-of-range `version`
/// still deserializes fine as plain data (both are ordinary `String` fields), so only this
/// function's semantic pass rejects them. Also rejected: an empty `path`, a duplicate `path` across
/// `files`, and a symbol whose `body_end` is less than its `body_start`.
///
/// Collects every applicable issue rather than stopping at the first — a producer fixing its output
/// against one `validate_envelope` call should see every problem at once, not one round-trip per bug
/// (the same "structured, list of issues" shape `pack_loader::load_dsl_packs`'s `LoadResult::errors`
/// uses for a directory of packs). Returns `Ok(envelope)` only when the JSON parses AND every semantic
/// check passes; a JSON parse failure short-circuits with a single-element `Vec` (there is no partial
/// envelope to inspect for further issues in that case).
///
/// This is the VALIDITY axis only. Shapes that are valid but almost certainly not what the producer
/// meant (an absolute `files[].path`, a non-normalized `http` key) are the separate ADVISORY axis —
/// [`envelope_hints`], returned alongside this result by [`validate_envelope_verdict`]. A caller that
/// only needs "may I analyze this" keeps using this function unchanged.
// Note: `const_map_fragment`/`procedure_router_fragments`/`router_mount_fragments` presence is never
// validated here — any fragment content a producer emits is accepted as-is (empty is always valid,
// per their `#[serde(default)]`). An unresolvable `Ref`/`Mount` specifier is a composition-time
// concern, silently skipped by the engine's assembly pass, not a validation-time rejection — "never
// guessed" per this crate's convention, but also never a hard error for a shape this validator cannot
// know is wrong.
pub fn validate_envelope(json: &str) -> Result<NormalizedEnvelope, Vec<String>> {
    validate_envelope_verdict(json).result
}

/// Both axes of envelope judgment for one JSON text: [`validate_envelope`]'s VALIDITY result plus the
/// advisory [`envelope_hints`] pass, which is why the two are computed in one place — an authoring
/// surface (`zzop validate-envelope`, the `validate_envelope` MCP tool) wants both from one read, and
/// getting them from two entry points would deserialize the same text twice.
///
/// The two axes are deliberately SEPARATE fields and must stay so: `result` decides acceptance
/// (`valid`, the CLI's exit code, whether `analyze_envelope` proceeds) and is computed from the
/// structural issues ALONE — no hint can ever reach it. A hint says "accepted, but this is probably not
/// what you meant"; promoting one to a rejection would reject envelopes that conform to the contract.
///
/// `hints` is empty whenever the text did not deserialize at all (there is no envelope to inspect), and
/// otherwise carries every hint the pass found — INCLUDING when `result` is `Err`, so a producer fixing
/// a structural issue sees the semantic ones in the same round-trip rather than one per fix.
pub struct EnvelopeVerdict {
    /// The validity verdict — exactly what [`validate_envelope`] returns.
    pub result: Result<NormalizedEnvelope, Vec<String>>,
    /// Advisory hints (see [`envelope_hints`]). Never affects `result`.
    pub hints: Vec<String>,
}

pub fn validate_envelope_verdict(json: &str) -> EnvelopeVerdict {
    let envelope = match parse_envelope(json) {
        Ok(envelope) => envelope,
        Err(errors) => {
            return EnvelopeVerdict {
                result: Err(errors),
                hints: Vec::new(),
            }
        }
    };
    let issues = structural_issues(&envelope);
    let hints = envelope_hints(&envelope);
    EnvelopeVerdict {
        result: if issues.is_empty() {
            Ok(envelope)
        } else {
            Err(issues)
        },
        hints,
    }
}

/// Deserialization half — everything that must succeed before there is an envelope to judge at all.
fn parse_envelope(json: &str) -> Result<NormalizedEnvelope, Vec<String>> {
    // A JSON ARRAY root is a special case: serde's derived `Deserialize` for a struct accepts a
    // sequence as well as a map (the positional-fields fallback other serde formats rely on), so a
    // top-level array is NOT rejected as "wrong shape" the way a string/number/bool/null root already
    // is (those hit the ordinary "invalid type: X, expected struct NormalizedEnvelope" branch, which is
    // clear on its own). Instead each array element gets deserialized against the next declared field
    // in turn, so `["a"]` against a `format: String` first field fails with a field-level type mismatch
    // ("invalid type: integer `1`, expected a string") that reads like ONE field is wrong rather than
    // "this isn't an envelope at all" — a blind field test hit exactly this passing a JSON array as
    // `envelopeJson`. Caught here, before the struct deserialize, with the honest diagnosis.
    if matches!(
        serde_json::from_str::<serde_json::Value>(json),
        Ok(serde_json::Value::Array(_))
    ) {
        return Err(vec![
            "expected a JSON object envelope, got an array".to_string()
        ]);
    }
    serde_json::from_str(json).map_err(|e| vec![format!("invalid JSON: {e}")])
}

/// The VALIDITY pass: every semantic check whose failure makes the envelope unusable. Kept apart from
/// [`envelope_hints`] because the two answer different questions — this one rejects, that one advises.
fn structural_issues(envelope: &NormalizedEnvelope) -> Vec<String> {
    let mut errors = Vec::new();

    if envelope.format != NORMALIZED_AST_FORMAT {
        errors.push(format!(
            "unknown format: '{}' (expected '{NORMALIZED_AST_FORMAT}')",
            envelope.format
        ));
    }
    // The version is a RELEASE number (see `NORMALIZED_AST_CONTRACT_VERSION`), so acceptance is one
    // comparison: anything at or below this build's own version is a shape it knows. Rejecting newer is
    // the "never guess" half — an engine that kept going would silently drop the fields it does not
    // recognize and leave the producer believing they applied.
    let declared_version = parse_contract_version(&envelope.version);
    match declared_version {
        None => errors.push(format!(
            "malformed version: '{}' (expected MAJOR.MINOR.PATCH, e.g. \"{NORMALIZED_AST_CONTRACT_VERSION}\" — the release whose envelope shape these bytes conform to)",
            envelope.version
        )),
        Some(declared) => {
            let supported = parse_contract_version(SUPPORTED_NORMALIZED_AST_VERSION)
                .expect("this build's own CARGO_PKG_VERSION must be MAJOR.MINOR.PATCH");
            if declared > supported {
                errors.push(format!(
                    "unsupported version: {} (this engine is {SUPPORTED_NORMALIZED_AST_VERSION} and accepts envelopes up to its own version)",
                    envelope.version
                ));
            }
        }
    }

    let mut seen_paths: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for (idx, file) in envelope.files.iter().enumerate() {
        if file.path.is_empty() {
            errors.push(format!("files[{idx}]: empty path"));
        } else if !seen_paths.insert(file.path.as_str()) {
            errors.push(format!("files[{idx}] ('{}'): duplicate path", file.path));
        }
        for sym in &file.symbols {
            if let (Some(start), Some(end)) = (sym.body_start, sym.body_end) {
                if end < start {
                    errors.push(format!(
                        "files[{idx}] ('{}') symbol '{}': body_end ({end}) < body_start ({start})",
                        file.path, sym.name
                    ));
                }
            }
        }

        // `overrides` — three rules, each closing a way the declaration could be believed without being
        // honoured. See `ProjectionOverrides` and `MIN_VERSION_FOR_OVERRIDES` for why each exists.
        //
        // (1) VERSION FLOOR. `FileProjection` has no `deny_unknown_fields`, so this exact envelope handed
        // to an engine built before `overrides` existed would deserialize, be ignored, and produce a run
        // where the adapter believes it displaced a native binding and the engine quietly did not. That
        // engine cannot detect its own blind spot; this one can, so it is the one that has to say so.
        if !file.overrides.imports.is_empty() {
            let floor = parse_contract_version(MIN_VERSION_FOR_OVERRIDES)
                .expect("MIN_VERSION_FOR_OVERRIDES must be MAJOR.MINOR.PATCH");
            if declared_version.is_some_and(|declared| declared < floor) {
                errors.push(format!(
                    "files[{idx}] ('{}'): `overrides` requires version >= {MIN_VERSION_FOR_OVERRIDES}, but this envelope declares {}. An engine older than that drops the field silently, so the same bytes would displace nothing there — declare the version whose shape you are writing.",
                    file.path, envelope.version
                ));
            }
        }
        // `calls` — the call-graph-edge channel is an external SUBMISSION, so it carries its own
        // validation the native producers never need (they construct `RawCall`s in-process). Two
        // shapes are rejected rather than repaired, because each has no honest repair:
        //
        // (1) VERSION FLOOR — same mechanism and rationale as `overrides` above (see
        // `MIN_VERSION_FOR_CALLS`): an engine predating the field drops it silently and its
        // call-graph rules stay quiet, a recall loss the producer cannot see. Only the engine that
        // understands the field can reject the mislabel, so it does.
        if !file.calls.is_empty() {
            let floor = parse_contract_version(MIN_VERSION_FOR_CALLS)
                .expect("MIN_VERSION_FOR_CALLS must be MAJOR.MINOR.PATCH");
            if declared_version.is_some_and(|declared| declared < floor) {
                errors.push(format!(
                    "files[{idx}] ('{}'): `calls` requires version >= {MIN_VERSION_FOR_CALLS}, but this envelope declares {}. An engine older than that drops the field silently, so the same bytes would light no call-graph rule there — declare the version whose shape you are writing.",
                    file.path, envelope.version
                ));
            }
            // (1b) NO `#` IN A CALLS-CARRYING PATH. The attribution check below verifies
            // `from_symbol` by whole-path prefix (`strip_prefix`), but the whole-graph resolver
            // buckets `from_symbol` at its FIRST `#` (`callgraph::build_symbol_graph`) — for a path
            // containing `#` the two machines disagree, so every call this file emits would resolve
            // under a truncated path it never declared (against no imports, no symbols). No honest
            // repair exists at the boundary (rewriting either machine's split is a guess about which
            // the producer meant), so the shape is rejected — and only for files that actually carry
            // calls, since a `#` path with no calls meets neither machine.
            if file.path.contains('#') {
                errors.push(format!(
                    "files[{idx}] ('{}'): `calls` are not accepted on a file whose path contains '#' — call attribution is checked against the whole path, but the call-graph resolver buckets `from_symbol` at its FIRST '#', so this file's calls would be resolved under the truncated path '{}' (a file this envelope never declared). Rename the path to carry calls.",
                    file.path,
                    file.path.split('#').next().unwrap_or_default()
                ));
            }
        }
        for (call_idx, call) in file.calls.iter().enumerate() {
            // (2) ATTRIBUTION — a call must belong to the file that emits it. `from_symbol` is the
            // grouping key the whole-graph resolver buckets by (`callgraph::build_symbol_graph` splits
            // it at the first `#`), so a foreign-file prefix would resolve this call against ANOTHER
            // file's imports — an edge minted under an attribution the emitting producer never
            // controlled. Guessing which file was meant has no honest form, so the boundary rejects.
            let well_attributed = call
                .from_symbol
                .strip_prefix(file.path.as_str())
                .is_some_and(|rest| rest.starts_with('#') && rest.len() > 1);
            if !well_attributed {
                errors.push(format!(
                    "files[{idx}] ('{}') calls[{call_idx}]: from_symbol '{}' must be '<this file's path>#<symbol>' — a call is attributed to a symbol of the file that emits it, never another file's.",
                    file.path, call.from_symbol
                ));
            }
            if call.callee_name.is_empty() {
                errors.push(format!(
                    "files[{idx}] ('{}') calls[{call_idx}]: empty callee_name",
                    file.path
                ));
            }
        }
        let mut seen_overrides: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for local_name in &file.overrides.imports {
            // (2) REPLACEMENT MANDATORY. A name declared without a binding to put in its place is a
            // deletion request. Deletion is refused at the contract boundary rather than the merge: it
            // has no honest output form (no replacement fact to disclose) and it would let an adapter
            // blind the engine without leaving a trace.
            if !file.imports.contains_key(local_name.as_str()) {
                errors.push(format!(
                    "files[{idx}] ('{}'): overrides.imports names '{local_name}', which this projection's `imports` does not bind. An override must SUPPLY the replacement; declaring one without it is a deletion, which this contract does not offer.",
                    file.path
                ));
            }
            // (3) NO DUPLICATES. A repeated name is not ambiguous in effect, but it makes the tombstone
            // census (one displacement disclosed per declaration) disagree with the declaration itself.
            if !seen_overrides.insert(local_name.as_str()) {
                errors.push(format!(
                    "files[{idx}] ('{}'): overrides.imports lists '{local_name}' more than once",
                    file.path
                ));
            }
        }
    }

    errors
}
