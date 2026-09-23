//! §33/§37 LANDING for `security/api-key-in-url`, whose remedy is "move the credential into an
//! `Authorization` header (or body/cookie)".
//!
//! WHAT THE READER'S CORRECT EDIT COSTS (`1.architecture/rules/rule-quality.md` §27 leg 3). This was
//! the shortest non-carrier in the tree at 343 characters, and §34's shape exactly: the imperative is
//! complete, so there was no grammatical slot for the two things it costs. First, changing WHERE a
//! credential travels breaks every caller that cannot carry it the new way, and the population is
//! larger than it looks — a webhook URL already registered in a vendor's console, an `<img>`/`<script>`
//! src, a signed link already handed out, a plain browser navigation. None of those can set a request
//! header, and the server that stops reading the query parameter stops authenticating all of them at
//! once. Second, and this is what turns a clean edit into a false sense of closure: moving the value
//! does not un-leak it. The whole reason this rule fires is that URLs are recorded, so the old value is
//! already in the proxy, CDN, browser-history and `Referer` logs that saw those URLs, and the edit
//! leaves every copy exactly where it is.
//!
//! WHY `ROTATION_LANDING` IS NOT REUSED — §37's most-dangerous-reuse test, and it is the closest
//! sibling in this pack because the second half of this sentence ENDS in a rotation. Its noun is the
//! credential's VALUE: issuing a replacement invalidates the old one at that instant, so every holder
//! not migrated starts failing auth. The noun here is the credential's TRANSPORT, and the two costs are
//! not the same cost — a caller can hold the right value and still fail, because it has no way to put
//! that value in a header. Splicing the rotation constant here would tell a reader to plan an
//! overlapping key window for a problem that an overlapping key window does not touch. The sentence
//! below therefore names rotation as a SECOND step rather than restating what it costs; that is
//! `ROTATION_LANDING`'s subject and its eight carriers keep it.
//!
//! NOT A DISQUALIFIER (§38's two axes). The finding does not become wrong because callers exist that
//! cannot send a header — the credential is still in a URL and the URL is still logged. The rule's
//! axis-A row in `message_order_verdicts.rs` stays a `NO_DISQUALIFIER` row; its CATEGORY gains
//! "+ landing" for the reason the three restored rows there record, which is that a row saying the
//! message carries only a suppression-marker note goes stale the moment a landing adds prose.
//!
//! POSITION, not presence. The invalidation probe is to move this constant behind the imperative —
//! there is exactly one sentence after it, the Python marker spelling, so the probe appends it to the
//! tail — where every token stays spelled once and only ORDER has changed.

use crate::{assert_landing_precedes_imperative, hits, scan, TempDir};

const CREDENTIAL_TRANSPORT_LANDING: &str = "CHANGING WHERE THE CREDENTIAL TRAVELS BREAKS EVERY CALLER THAT CANNOT CARRY IT THE NEW WAY, AND IT DOES NOT UN-LEAK THE OLD ONE: a webhook URL already registered in a vendor console, an `<img>`/`<script>` src, a signed link already handed out and a plain browser navigation can each send a query string and none of them can set a header, so keep accepting both while those callers move and stop reading the parameter only once nothing sends it. And this value is already sitting in the proxy, CDN, browser-history and `Referer` logs that recorded the old URLs, which is what this finding is about — so rotate it as part of this change rather than after it.";

/// The one carrier. `security/api-key-in-url` fires twice on cal.com in the dogfood corpus, so these
/// bytes reach a reader outside this fixture.
#[test]
fn api_key_in_url_landing_precedes_the_imperative() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "api/client.ts",
        "export const url = \"https://api.example.com/data?api_key=abc123\";\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "api-key-in-url");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 1);
    assert_landing_precedes_imperative(
        "api-key-in-url",
        &h[0].message,
        CREDENTIAL_TRANSPORT_LANDING,
        "Move the credential into an `Authorization` header",
    );
}
