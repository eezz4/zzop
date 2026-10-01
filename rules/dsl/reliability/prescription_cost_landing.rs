//! §27 LANDING for the two rules an uncontaminated first-screen audit judged HARMFUL — and they were
//! the two shortest prescriptive messages on their screens, which is the finding (review ledger V310).
//!
//! WHAT THE MEASUREMENT SAID. Three of the four HARMFUL verdicts across three trees came from
//! `reliability/sync-fs-in-handler` (immich, two rows) and `reliability/debug-true-committed` (nocodb
//! and cal.com, one each — two auditors who never saw each other's work, naming the same rule for the
//! same reason). Both wrote the reason in nearly the same words: every OTHER prescriptive rule on the
//! same screen put its breakage warning in FRONT of its imperative, and these two had none at all. The
//! cal.com auditor's sentence is the one worth keeping — what made the verdict was not the code, it
//! was the absence.
//!
//! Census over the shipped DSL packs the day this landed: 118 rules, 59 prescriptive, 10 with no
//! landing. Median message length across prescriptive rules 1,710 chars; across those ten, 680. These
//! two sat at 606 and 649 — second and fifth shortest. Brevity is not itself the defect; it is what
//! the missing paragraph looks like from outside.
//!
//! WHY THE TWO LANDINGS ARE NOT ONE CONSTANT (§37's most-dangerous-reuse test). They fail in different
//! places. `sync-fs-in-handler`'s cost is STRUCTURAL: the read may sit in a factory that RETURNS the
//! handler, so it runs once at boot, and awaiting it hands `app.use()` a Promise instead of a function.
//! `debug-true-committed`'s cost is about what the LINE IS: `rejectUnauthorized: false` can be a value
//! inside a table something reads at runtime rather than configuration for this process, and deleting
//! it breaks the feature that reads it. One is about where code runs, the other about whether a line is
//! code at all. A shared constant would have to say both, and would say neither.
//!
//! POSITION, not presence. The invalidation probe for both: move the landing behind the imperative with
//! every token still spelled exactly once — each pin must go red on ORDER alone.

use crate::{assert_landing_precedes_imperative, hits, scan, TempDir};

/// immich `c9776fb99` had two of these and the auditor called both HARMFUL: the `readFileSync` sat in
/// an `ssr(paths)` factory whose return value is the middleware, so the prescription's `await` would
/// have thrown at boot. The fixture here is the rule's own positive case — the landing has to reach a
/// reader of the ORDINARY finding, not only of the awkward one.
#[test]
fn sync_fs_in_handler_landing_precedes_the_async_imperative() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/handler.ts",
        "import { readFileSync } from \"fs\";\nexport function handler(req: any, res: any) {\n  const data = readFileSync(\"./config.json\", \"utf8\");\n  res.status(200).json(data);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "sync-fs-in-handler");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_landing_precedes_imperative(
        "reliability/sync-fs-in-handler",
        &h[0].message,
        "BEFORE YOU MAKE IT ASYNC",
        "Use the async `fs.promises`",
    );
}

/// cal.com `176037d0`: the flagged `rejectUnauthorized: false` was an element of a config-FIX table the
/// connection wizard APPLIES when an Exchange server presents a self-signed certificate. Removing it
/// does not harden a deployment; it makes that wizard unable to connect. nocodb produced the same
/// verdict on a different shape the same day.
#[test]
fn debug_true_committed_landing_precedes_the_remove_imperative() {
    let dir = TempDir::new("zzop-be-rel");
    dir.write(
        "src/config.ts",
        "export const config = { debug: true, name: \"app\" };\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "debug-true-committed");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_landing_precedes_imperative(
        "reliability/debug-true-committed",
        &h[0].message,
        "BEFORE YOU REMOVE IT",
        "Remove the flag, or gate it",
    );
}
/// POSITION, for the third rule this measurement reached (2026-09-25, review ledger V378).
///
/// `reliability/interval-no-clear` already had an ORDER pair in `message_order_verdicts.rs` and it
/// was green while the condition sat 26% into the message. The audit that produced these landings
/// measured the reader stopping at row 15, so "before the imperative" is not the property worth
/// pinning — "first" is. Reorder only: no token added, none removed.
#[test]
fn the_interval_file_scope_condition_is_the_first_sentence() {
    let pack: serde_json::Value =
        serde_json::from_str(include_str!("reliability.json")).expect("reliability.json parses");
    let message = pack["rules"]
        .as_array()
        .expect("rules array")
        .iter()
        .find(|r| r["id"].as_str() == Some("interval-no-clear"))
        .and_then(|r| r["message"].as_str())
        .expect("interval-no-clear carries a message")
        .to_string();

    const HEAD: &str = "WHEN THIS FINDING IS WRONG: the search is FILE-SCOPED";
    assert!(
        message.starts_with(HEAD),
        "reliability/interval-no-clear: the file-scope condition must be the message's FIRST \
         sentence — an interval handed to a caller in another file is indistinguishable from a leak \
         here, and a reader who acts on the imperative first deletes a timer someone else cancels. \
         It starts at byte {:?} instead. In: {message}",
        message.find(HEAD)
    );
}
