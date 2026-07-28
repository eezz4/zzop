//! Envelope SEMANTIC hints — the advisory half of envelope judgment (`crate::normalized`'s validity
//! pass is the other half). A hint says "this envelope is accepted, but this is probably not what you
//! meant"; it NEVER makes a valid envelope invalid (see [`crate::normalized::EnvelopeVerdict`], where
//! `result` is computed from the structural issues alone and this module's output rides beside it).
//!
//! Why these four and no more: each one is a shape that parses fine, analyzes fine, and then quietly
//! costs the producer something the envelope's `"valid": true` never mentions — the silent-failure class
//! this repo treats as its worst failure mode. Absolute `files[].path`s are the archetype: the envelope
//! validates, and as a Mode B overlay every one of those paths matches no file in the tree, so nothing
//! merges onto the real file it names. Each message below states only the consequence its own mode
//! actually produces — an overstated consequence teaches a producer to distrust the whole pass.
//!
//! Restored from the JS CLI's `lintEnvelope` (removed with the npm distribution, `285677a`), but the
//! `http` normal-form judgment is no longer a hand-written `^[A-Z]+ /` regex: it round-trips the key
//! through the SAME [`http_interface_key`]/[`http_consume_interface_key`] the join itself keys on, so
//! this module cannot drift from the normalization it is checking against, and the hint can name the
//! exact key the producer should have emitted instead of just the shape it missed.

use std::collections::HashSet;

use crate::io::{http_consume_interface_key, http_interface_key};
use crate::normalized::NormalizedEnvelope;

/// Which side of the join a key came from — the two sides normalize DIFFERENTLY and the difference is
/// contractual (see [`http_consume_interface_key`]'s doc: a `?` is a query separator at a call site but
/// a legitimate pattern character in a route provide), so the check has to know which one it is holding.
#[derive(Clone, Copy)]
enum Side {
    Provide,
    Consume,
}

impl Side {
    /// The core helper a producer of this side's keys is supposed to have used — named in the hint so
    /// the fix is a lookup, not a guess.
    fn keying_helper(self) -> &'static str {
        match self {
            Side::Provide => "zzop_core::io::http_interface_key",
            Side::Consume => "zzop_core::io::http_consume_interface_key",
        }
    }
}

/// Advisory hints for an already-deserialized envelope: four checks over the `io` entries and
/// `files[].path`, each one a known cause of a silently empty join. Returns an empty `Vec` when nothing
/// looks suspicious — the normal result for a conforming envelope, including one that emits no `io` at
/// all (absence is never a hint; a hint requires positive evidence).
///
/// ORDER is deterministic by construction: `files` in declared order, and within a file every provide
/// then every consume in declared order. The one `HashSet` here is used for membership only, never
/// iterated, so no hint's position depends on hashing.
pub fn envelope_hints(envelope: &NormalizedEnvelope) -> Vec<String> {
    let mut hints = Vec::new();
    // (kind, key, file, line) — the duplicate-provide identity, tree-wide: an adapter that emits the
    // same route twice from two passes usually does it across files, not within one.
    let mut seen_provides: HashSet<(&str, &str, &str, u32)> = HashSet::new();

    for (idx, file) in envelope.files.iter().enumerate() {
        if looks_absolute(&file.path) {
            hints.push(format!(
                "files[{idx}].path '{}' is an absolute path — an envelope carries TREE-RELATIVE paths. \
                 Findings, dep-graph nodes and loc are keyed on this path verbatim, so they report the \
                 producer's machine layout; and as a Mode B overlay it matches no file in the tree at \
                 all, so this projection is added as a separate synthetic entry instead of merging onto \
                 the file it names. Emit it relative to the tree root, e.g. 'src/foo.ts'.",
                file.path
            ));
        }

        for provide in &file.io.provides {
            let at = format!("files[{idx}] provide at {}:{}", provide.file, provide.line);
            if provide.key.contains("://") {
                // Checked BEFORE (and instead of) the normal-form round-trip: a host-carrying key is
                // never canonical either, and reporting both would state one defect twice with the
                // less precise sentence second.
                hints.push(format!(
                    "{at}: key '{}' carries a host ('://') — a provide keys THIS tree's own interface \
                     path, and host-carrying keys are consume-side external egress only (they are \
                     bucketed as third-party egress, never joined), so nothing can ever resolve to \
                     this provide; drop the scheme and authority.",
                    provide.key
                ));
            } else if let Some(hint) = http_key_hint(&provide.kind, &provide.key, Side::Provide) {
                hints.push(format!("{at}: {hint}"));
            }
            if !seen_provides.insert((
                provide.kind.as_str(),
                provide.key.as_str(),
                provide.file.as_str(),
                provide.line,
            )) {
                hints.push(format!(
                    "{at}: duplicate provide — kind '{}' key '{}' is emitted more than once at the \
                     identical location; remove the duplicate entry (nothing folds the copies together \
                     on the whole-tree envelope path, so each one is joined separately: every consume \
                     of this key gets a second identical cross-layer edge, and an unconsumed one is \
                     listed twice in unconsumedProvides).",
                    provide.kind, provide.key
                ));
            }
        }

        for consume in &file.io.consumes {
            // `key: None` is the DOCUMENTED shape of a consume the adapter could not statically
            // resolve (`IoConsume::key`) — reported as an unresolved consume by the engine, not a
            // keying mistake, so it is out of this check's evidence.
            let Some(key) = consume.key.as_deref() else {
                continue;
            };
            // An absolute-URL consume key is legitimate on this side (`crate::io`'s external-egress
            // gate exists for exactly it), so the normal-form round-trip has nothing to say: running
            // it would report the supported shape as a mistake.
            if key.contains("://") {
                continue;
            }
            if let Some(hint) = http_key_hint(&consume.kind, key, Side::Consume) {
                hints.push(format!(
                    "files[{idx}] consume at {}:{}: {hint}",
                    consume.file, consume.line
                ));
            }
        }
    }

    hints
}

/// The `http` normal-form check, evidence-gated on `kind` being literally `"http"` — the `"METHOD /path"`
/// vocabulary is that kind's alone, and applying it to an adapter's own kind (`db-table`, a topic, an
/// env key) would be inventing a contract that kind never had.
///
/// Judgment = "would the core keying helper have produced this exact string" rather than a shape regex:
/// it catches a lowercase verb, a missing leading slash, an unsubstituted `:id`/`{id}` param, a doubled
/// or trailing slash, and (consume side) a query suffix — every one of which misses the exact-key join
/// while passing `^[A-Z]+ /`.
fn http_key_hint(kind: &str, key: &str, side: Side) -> Option<String> {
    if kind != "http" {
        return None;
    }
    let helper = side.keying_helper();
    let Some((method, path)) = key.split_once(' ') else {
        return Some(format!(
            "http key '{key}' is not the normalized 'METHOD /path' form (^[A-Z]+ /, e.g. \
             'GET /users/{{}}') — it has no method/path split at all, so it can never join a key from \
             the other side; produce it with {helper}."
        ));
    };
    let canonical = match side {
        Side::Provide => http_interface_key(method, path),
        Side::Consume => http_consume_interface_key(method, path),
    };
    (canonical != key).then(|| {
        format!(
            "http key '{key}' is not the normalized 'METHOD /path' form — the join is an EXACT key \
             match, so this joins nothing; emit '{canonical}' instead (that is what {helper} produces \
             for it)."
        )
    })
}

/// True when `path` looks like an absolute filesystem path rather than a tree-relative one: a POSIX
/// root (`/…`), a Windows drive (`C:\…`/`C:/…`), or a UNC path (`\\host\share`, covered by the leading
/// separator arm).
///
/// Deliberately NOT `std::path::Path::is_absolute`: an envelope is WIRE data that a Linux adapter may
/// hand to a Windows engine and vice versa, and `is_absolute` answers for the HOST platform only
/// (`/etc/x` is not "absolute" on Windows), which would make the same envelope hint differently
/// depending on who validated it. Kept private for the same reason `crate::paths` does not gain a row:
/// this is a wire-shape predicate with one consumer, not a shared analysis predicate.
fn looks_absolute(path: &str) -> bool {
    match path.as_bytes() {
        [b'/' | b'\\', ..] => true,
        [drive, b':', b'/' | b'\\', ..] => drive.is_ascii_alphabetic(),
        _ => false,
    }
}
