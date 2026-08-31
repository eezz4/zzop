//! §33/§37 LANDING for the two rules that offer `Promise.allSettled(...)` as a way out —
//! `promise-all-and-writes` and `map-async-no-promise-all`.
//!
//! THIS FILE STARTED AS A SIBLING MISMATCH, not as a new sentence. `promise-all-and-writes` has
//! carried the whole warning below since 2026-08-25: swapping in `Promise.allSettled(...)` changes
//! what the awaited array RESOLVES TO — settlement wrappers rather than the values — so every
//! downstream reader gets a wrapper where it expected a record. `map-async-no-promise-all` prescribed
//! the same swap in a parenthetical ("or `Promise.allSettled(...)` for expected partial failures")
//! and said nothing at all: 301 characters, no limitation prose of any kind. One sibling already owned
//! the sentence the other needed.
//!
//! THE REPAIR IS A CONSTANT, NOT A COPY. Duplicating the prose would put two authored copies of one
//! claim in the tree and guarantee that one of them goes stale. Promoting it to a `const` gives the
//! family ONE spelling, makes both messages carry those exact bytes, and lets the registry in
//! `message_order_verdicts.rs` see both as carriers — which is also how the mismatch surfaced. The
//! constant's text is `promise-all-and-writes`' message verbatim, so that rule's shipped bytes did not
//! change at all; it gained a position pin it never had.
//!
//! WHAT REGISTERING IT FOUND, recorded because it is the reason axis B exists. With the constant
//! declared and no pin written, `every_dsl_rule_carries_a_landing_verdict` went red naming
//! `reliability/promise-all-and-writes` — a landing that had been shipping unpositioned for five days
//! because nothing had registered its spelling. The guard cannot see prose; it can only see constants.
//!
//! WHY THIS IS A LANDING AND NOT A DISQUALIFIER (§38's two axes). Neither finding becomes wrong when
//! the resolved array is consumed. `.map(async ...)` without a wrapper still leaks rejections; the
//! writes still land non-atomically. What changes is that one of the two remedies offered costs the
//! reader a silent type change at every consumer, and the compiler catches it only where the value was
//! annotated. Both rules keep their existing axis-A verdicts untouched.
//!
//! POSITION, not presence. The invalidation probe for both tests is to move this constant to the tail
//! of the message: every token stays present and spelled exactly once, and each pin goes red on ORDER.

use crate::{assert_landing_precedes_imperative, hits, scan, TempDir};

const ALLSETTLED_WRAPPER_LANDING: &str = "IF ANY CODE CONSUMES THE RESOLVED ARRAY, DO NOT SWAP IN `Promise.allSettled(...)`: it resolves to settlement wrappers (`{ status, value }` / `{ status, reason }`) rather than to the values themselves, so every downstream reader gets a wrapper where it expected a record. Whether anything catches that depends on the consumer, and the compiler is the good case — an array that flows into an annotated field fails to build, while the swap is silent exactly where the value is `any`, untyped, or merely spread into a log line or a response body.";

/// The rule that already owned the sentence. Its message is unchanged by this batch — what it gains is
/// the position pin, in a `fn` of its own.
///
/// SEPARATE `fn` ON PURPOSE, and this is a trap worth naming. `delivered_pins` in
/// `message_order_verdicts.rs` skips its inline-comparison scan for any block that already calls a
/// shared helper (`if any_helper { continue; }`). This rule's axis-A verdict IS an inline comparison,
/// in `writes_and_parsing.rs`; adding a landing helper call to that same `fn` would have made the
/// block "helper-bearing", dropped the inline pin from the scan, and left the rule with no axis-A
/// verdict at all — a green edit that silently deletes a declaration.
#[test]
fn promise_all_and_writes_landing_precedes_the_imperative() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/save.ts",
        "declare const db: any;\nexport async function saveAll(items: any[]) {\n  return Promise.all(items.map((i) => db.record.create({ data: i })));\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "promise-all-and-writes");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_landing_precedes_imperative(
        "promise-all-and-writes",
        &h[0].message,
        ALLSETTLED_WRAPPER_LANDING,
        "Wrap the writes in one transaction instead",
    );
}

/// The rule that needed it. The landing sits ahead of the imperative that offers the swap, so the
/// parenthetical "(or `Promise.allSettled(...)` for expected partial failures)" is now read by someone
/// who has already been told what the settled form resolves to.
#[test]
fn map_async_no_promise_all_landing_precedes_the_imperative() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/process.ts",
        "declare function fetchItem(id: string): Promise<unknown>;\nexport async function process(ids: string[]) {\n  return ids.map(async (id) => {\n    return await fetchItem(id);\n  });\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "map-async-no-promise-all");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_landing_precedes_imperative(
        "map-async-no-promise-all",
        &h[0].message,
        ALLSETTLED_WRAPPER_LANDING,
        "Wrap the call in `Promise.all(...)`",
    );
}

/// BYTE-IDENTITY, asserted rather than assumed. This is the whole value of promoting the sentence to a
/// constant: two messages, one spelling. A paraphrase in either would still read fine to a human and
/// would silently un-index the position pin, which is the drift this assertion exists to catch.
#[test]
fn both_carriers_splice_the_same_bytes() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/save.ts",
        "declare const db: any;\nexport async function saveAll(items: any[]) {\n  return Promise.all(items.map((i) => db.record.create({ data: i })));\n}\n",
    );
    dir.write(
        "src/process.ts",
        "declare function fetchItem(id: string): Promise<unknown>;\nexport async function process(ids: string[]) {\n  return ids.map(async (id) => {\n    return await fetchItem(id);\n  });\n}\n",
    );
    let out = scan(&dir);
    let a = hits(&out, "promise-all-and-writes");
    let b = hits(&out, "map-async-no-promise-all");
    assert_eq!(a.len(), 1, "{:?}", out.findings);
    assert_eq!(b.len(), 1, "{:?}", out.findings);
    for (rule, m) in [
        ("promise-all-and-writes", &a[0].message),
        ("map-async-no-promise-all", &b[0].message),
    ] {
        assert_eq!(
            m.matches(ALLSETTLED_WRAPPER_LANDING).count(),
            1,
            "{rule}: the shared landing must appear exactly once, byte-identical. In: {m}"
        );
    }
}
