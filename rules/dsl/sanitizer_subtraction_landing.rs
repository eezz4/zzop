//! §33/§37 LANDING for the seven rules whose remedy is "sanitize it, or render it as text" —
//! `browser/{unsafe-html-sink,jquery-html-sink,vue-v-html,markdown-and-html-sink-unsanitized}` and
//! `security/{dangerous-html-concat,html-response-from-request,template-unescaped-output}`.
//!
//! NOT A PACK DIRECTORY, and the only landing constant in this tree that is not. The seven span TWO
//! packs, each pack root is its own `[[test]]` target — a separate crate root — so a constant declared
//! in either pack cannot be reached from the other, and duplicating it is the one thing §37 forbids
//! (one spelling, byte-identical, because a position pin needs one spelling to index). So the constant
//! lives here beside `message_order_pins.rs`, which each pack root already pulls in the same way, and
//! is reached through the accessor below because the coverage guard's registry scanner reads only a
//! BARE single-line `const NAME: &str = "...";` — a `pub(crate) const` is invisible to it, and an
//! invisible constant makes all seven carriers read as non-carriers on axis B.
//!
//! WHAT THE READER'S CORRECT EDIT COSTS (`1.architecture/rules/rule-quality.md` §27 leg 3). Every one
//! of the seven ends in the same move: put the value through a sanitizer, or through a substitute that
//! renders it as text (`textContent`, jQuery `.text()`, Vue `{{ }}`, an escaped template form). Both
//! halves of that move are SUBTRACTIVE and neither raises. A sanitizer is an allow-list, so it deletes
//! what it was not told to keep — the defaults differ per library and are narrower than authored
//! content usually is — and the text substitutes are not selective at all: markup arrives on the page
//! as characters. The failure is a page that renders and is wrong, discovered by a person looking at
//! it rather than by anything that logs.
//!
//! ONE CONSTANT FOR THE SEVEN, byte-identical. The property belongs to sanitization as such, not to
//! what any one rule detects: an `innerHTML` assignment, a jQuery `.html()` call, a `v-html` binding, a
//! markdown render, a hand-built response string and a triple-stache all reach the same allow-list, so
//! the cost is the same sentence on the client and on the server. The rules diverge only in their
//! EXIT, which is each rule's own imperative and stays in each rule's own message. Same split
//! `ROTATION_LANDING` uses for its eight and `BOUND_PARAMETER_LANDING` for its three.
//!
//! WHY `browser/no-document-write` IS NOT AN EIGHTH CARRIER, and it is the closest sibling in this
//! batch — §37's most-dangerous-reuse test. Its remedy names `textContent` too, so half of this
//! sentence is true there. Its DOMINANT cost is not: `document.write` splices into the token stream at
//! the parser's current position, and what breaks is a parent that does not exist yet and a script that
//! stops being parser-blocking. A landing whose noun is an allow-list would be shipped to a reader who
//! is not calling a sanitizer. It carries `DOCUMENT_WRITE_TIMING_LANDING` in `browser/dialogs.rs`
//! instead, and that constant states the `textContent` overlap in its own bytes.
//!
//! WHY `browser/javascript-url` IS NOT ONE EITHER. Its remedy is a SCHEME test, not a sanitizer: what
//! it over-rejects is relative, `mailto:` and `blob:` targets, and what deleting the flagged attribute
//! costs is an anchor that stops being a link. Different noun, different failure; it carries
//! `URL_SCHEME_ALLOWLIST_LANDING` in `browser/javascript_url.rs`.
//!
//! WHY `security/taint-flow` IS NOT AN EIGHTH CARRIER EITHER, recorded here so the next author does not
//! "complete the family" by pasting this in. Its third remedy IS this move, spelled with this noun, so
//! the constant would be true there. But that rule hands the reader three remedies for three different
//! sinks — parameterize the SQL, take the shell out, use `textContent` — and only the third would get a
//! landing. A message that names the cost of one instruction out of three reads as landed, which is
//! the state that stops the next author from asking about the other two. Its siblings for those two
//! already exist (`BOUND_PARAMETER_LANDING`, `SHELL_ROUTING_LANDING`) and are keyed to other languages'
//! spellings, so it is a rule-sized decision rather than a splice. The backlog owns it.
//!
//! NOT A DISQUALIFIER, which is why these are pinned with the landing helper rather than a clause one.
//! A reader whose value is rich text authored by a user still has a real finding — that string still
//! reaches an HTML parser. What changes is that the remedy has a configuration step they were not told
//! about, and a wrong default costs them content rather than an error.
//!
//! POSITION, not presence. The invalidation probe for every carrier is to move this constant to the
//! tail of that rule's message: every token stays present and spelled exactly once, and each pin must
//! go red on ORDER alone.

/// The sanitizer-subtraction landing, spliced ahead of the sanitize/render-as-text imperative in all
/// seven messages. Not a `convention` value — no project declares it; it is zzop's own sentence about
/// what the reader's own edit does to content that is already on the page.
///
/// The two library defaults it names are the ones the seven messages themselves name, and they are
/// stated as defaults rather than as behaviour: both are configuration, both have moved across major
/// versions, and the sentence's own instruction is to read the list that ships rather than this one.
const SANITIZER_SUBTRACTION_LANDING: &str = "A SANITIZER IS AN ALLOW-LIST, SO IT DELETES WHATEVER IT WAS NOT TOLD TO KEEP, AND THE PAGE STILL RENDERS: nothing throws and nothing is logged, so the loss arrives as an embed that is simply gone, a link that stopped opening in a new tab, or a block that lost the attribute it was styled by. The lists are library defaults rather than a standard, and they are narrower than authored content usually is — `sanitize-html` ships a short tag list holding no `iframe`, `svg`, `video` or `form` and grants attributes only to `a` and `img`, so `class`, `style` and `id` come off the elements it does keep, while DOMPurify keeps far more and still drops `iframe` and the `target` attribute until they are added back. The plain-text route is not selective at all: `textContent`, jQuery `.text()`, Vue `{{ }}` and an escaped template form print markup as characters, so a stored `<b>bold</b>` reaches the reader as that literal text rather than as bold type. Settle which of the two this value is BEFORE the edit — where it is text the substitution is right and costs nothing, and where it is markup somebody authored, sample what this sink receives today, configure the allow-list against that sample, and keep the configuration in one place instead of per call site.";

/// The one spelling, handed to the two pack roots that carry it.
///
/// An accessor rather than a `pub(crate) const` on purpose: the axis-B registry in
/// `message_order_verdicts.rs` scans source for a bare `const NAME: &str = "...";` and skips anything
/// with a visibility prefix, so making the constant itself reachable would take it out of the registry
/// and make every carrier read as carrying no landing at all.
pub(crate) fn sanitizer_subtraction_landing() -> &'static str {
    SANITIZER_SUBTRACTION_LANDING
}
