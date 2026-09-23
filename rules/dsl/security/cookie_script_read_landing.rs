//! §37 LANDING for `security/insecure-cookie` — adding `httpOnly` SUCCEEDS, and what it costs is every
//! client reader of that cookie, silently.
//!
//! WHY (`1.architecture/rules/rule-quality.md` §27 leg 3). Hiding the cookie from scripts is the whole
//! point of the flag, so any client code that reads it by name stops seeing it — and stops without
//! throwing: a helper scanning `document.cookie` returns `undefined` for a hidden cookie exactly as it
//! does for an absent one. Nothing fails at the edit; the loss arrives later as a logged-out user or a
//! request missing a header. That is why the sentence ends by telling the reader to search their client
//! for the cookie's name FIRST — the check has to happen before the edit, not after the incident.
//!
//! WHY IT TAKES ITS OWN CONSTANT (§37's most-dangerous-reuse test). `CREDENTIAL_TRANSPORT_LANDING` is
//! the closest sibling in this pack — it is also about moving a credential out of a place readers can
//! see — but it names holders that CANNOT move (a webhook URL registered with a third party, an `img`
//! src, a signed link) and its subject is the credential's transport. This one's subject is a reader in
//! the same document that simply stops finding the value, and the remedy when that reader is legitimate
//! is to SPLIT the cookie rather than to migrate a holder.
//!
//! THE OTHER AXIS IS ALREADY PINNED AND STAYS THERE (§38). This rule's §27-a pin in `http_exposure.rs`
//! uses a fragment of this same sentence as its planted summary. That is a known mis-attribution of the
//! kind the two-axis split was built to expose, and repairing it is an axis-A decision with a floor
//! attached; registering the landing here neither creates nor removes it, and the pin below is a
//! separate `#[test]` so the two claims stay separately readable.
//!
//! POSITION, not presence. The invalidation probe is to move this constant to the tail of the message:
//! every token stays present and spelled exactly once, and the pin must go red on ORDER alone.

use crate::{assert_landing_precedes_imperative, hits, scan, TempDir};

/// The cookie-script-read landing, spliced ahead of the `{ httpOnly: true }` imperative.
const COOKIE_SCRIPT_READ_LANDING: &str = "HIDING IT FROM SCRIPTS IS WHAT THE FLAG DOES, SO ANY CLIENT CODE THAT READS THIS COOKIE BY NAME STOPS SEEING IT — AND STOPS SILENTLY: a helper that scans `document.cookie` returns `undefined` for a hidden cookie exactly as it does for an absent one, so nothing throws at the edit and the loss surfaces later as a logged-out user or a request missing a header. Search your client for this cookie's name before making the change.";

/// One constant, one carrier — asserted against the shipped pack so a sibling that grows a cookie-flag
/// remedy and pastes this sentence fails here rather than shipping a second spelling.
#[test]
fn the_cookie_script_read_landing_is_carried_by_exactly_one_rule() {
    let pack: serde_json::Value =
        serde_json::from_str(include_str!("security.json")).expect("security.json parses");
    let mut carriers: Vec<&str> = pack["rules"]
        .as_array()
        .expect("rules array")
        .iter()
        .filter(|r| {
            r["message"]
                .as_str()
                .is_some_and(|m| m.contains(COOKIE_SCRIPT_READ_LANDING))
        })
        .map(|r| r["id"].as_str().expect("rule id"))
        .collect();
    carriers.sort_unstable();
    assert_eq!(
        carriers,
        ["insecure-cookie"],
        "the cookie-script-read landing is carried by a different rule set than the one it was written for"
    );
}

/// The position pin, on a DELIVERED finding. Separate `#[test]` from this rule's §27-a pin in
/// `http_exposure.rs`: that one plants a 62-byte fragment of this sentence, which is satisfied by a
/// message that has kept the fragment and rewritten everything around it.
#[test]
fn insecure_cookie_landing_precedes_the_imperative() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/auth.ts",
        "declare const res: any;\ndeclare const token: string;\nexport function login() {\n  res.cookie(\"session\", token);\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "insecure-cookie");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_landing_precedes_imperative(
        "insecure-cookie",
        &h[0].message,
        COOKIE_SCRIPT_READ_LANDING,
        "IF THE SERVER IS THE ONLY READER, add `{ httpOnly: true }`",
    );
}
