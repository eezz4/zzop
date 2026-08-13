//! `console-in-be` — moved here with the rule from `rules/dsl/reliability/server_hygiene.rs`, which
//! keeps `body-limit-missing`, `interval-no-clear`, `stream-open-no-close-in-loop`,
//! `listener-subscribe-in-loop` and `fs-in-loop-serial`. The rule shared no fixture with any of them, so
//! this was a clean cut rather than a duplication.
//!
//! Its structural sibling `console-in-loop` moved too (`console_in_loop.rs`, whole file), as did the
//! wave-2 language coverage both rules share (`w2_languages.rs`) — so the pair that reads the
//! `console-write` call-site family is intact in one pack, and neither half lost the other as its
//! contrast.

use crate::{hits, scan, TempDir};

// --- console-in-be ---

#[test]
fn console_log_under_api_directory_is_flagged() {
    let dir = TempDir::new("zzop-hygiene");
    dir.write("src/api/handler.ts", "console.log(\"hit\");\n");
    let out = scan(&dir);
    let h = hits(&out, "console-in-be");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 1);
}

#[test]
fn console_log_outside_backend_directories_is_not_flagged() {
    let dir = TempDir::new("zzop-hygiene");
    dir.write("src/utils/logger.ts", "console.log(\"hit\");\n");
    let out = scan(&dir);
    assert!(hits(&out, "console-in-be").is_empty(), "{:?}", out.findings);
}

#[test]
fn console_ok_marker_above_the_line_suppresses_the_finding() {
    let dir = TempDir::new("zzop-hygiene");
    dir.write(
        "src/api/handler.ts",
        "// zzop-console-in-be-ok: temporary trace, removed before merge\nconsole.log(\"hit\");\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "console-in-be").is_empty(), "{:?}", out.findings);
}

// --- console-in-be after the call-scan migration ---
//
// The three tests above are the PARITY set: they passed against the line-scan version and must keep
// passing, which is what makes "this migration changed nothing on TypeScript" a measured claim rather
// than an intention. The tests below pin what the migration DID change, in both directions.

#[test]
fn a_python_print_on_a_backend_path_now_fires() {
    // The gain, and the only intended one on this rule. One rule, two languages — `print` is a
    // `console-write` with the callee spelled as Python spells it, reached by adding `py` to the file
    // pattern rather than by writing a second regex.
    let dir = TempDir::new("zzop-hygiene");
    dir.write("src/api/handler.py", "def handle():\n    print(\"hit\")\n");
    let out = scan(&dir);
    let h = hits(&out, "console-in-be");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 2);
}

#[test]
fn a_python_hash_ok_marker_suppresses_the_finding() {
    // `#` is a real leader for this matcher (io-scan's set, not line-scan's `//`-only), so a Python
    // author has the same escape hatch a TypeScript one does. Without this the rule would be
    // unsuppressable in the language it just reached.
    let dir = TempDir::new("zzop-hygiene");
    dir.write(
        "src/api/handler.py",
        "def handle():\n    # zzop-console-in-be-ok: startup banner, vetted\n    print(\"hit\")\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "console-in-be").is_empty(), "{:?}", out.findings);
}

#[test]
fn debug_and_trace_are_projected_but_stay_out_of_this_rule() {
    // The deliberate non-widening, pinned as a decision rather than left as an accident. The producer
    // emits all six `console` write methods (`CONSOLE_WRITE_METHODS`); this rule's callee pattern keeps
    // the four the text regex already flagged, so the migration's measured detection delta is
    // attributable to the Python reach alone. `console-in-loop` is where the other two are read.
    let dir = TempDir::new("zzop-hygiene");
    dir.write(
        "src/api/handler.ts",
        "console.debug(\"d\");\nconsole.trace(\"t\");\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "console-in-be").is_empty(), "{:?}", out.findings);
}

#[test]
fn a_console_call_named_only_in_a_string_or_a_comment_is_not_a_site() {
    // The parse dividend. The line-scan version needed `skip_comment_lines` for half of this and had no
    // answer at all for the other half; here neither is a site, so neither reaches the rule.
    let dir = TempDir::new("zzop-hygiene");
    dir.write(
        "src/api/handler.ts",
        "// console.log(\"commented out\")\nexport const help = \"call console.log(x) to debug\";\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "console-in-be").is_empty(), "{:?}", out.findings);
}

#[test]
fn a_structured_logger_is_never_a_console_write() {
    // The false fold the channel refuses at the producer, asserted from the rule side: a rule banning
    // console writes in a backend is not banning logging, in either language.
    let dir = TempDir::new("zzop-hygiene");
    dir.write(
        "src/api/handler.ts",
        "declare const logger: { info(m: string): void };\nexport function handle() {\n  logger.info(\"hit\");\n}\n",
    );
    dir.write(
        "src/api/svc.py",
        "import logging\n\n\ndef handle():\n    logging.info(\"hit\")\n",
    );
    let out = scan(&dir);
    assert!(hits(&out, "console-in-be").is_empty(), "{:?}", out.findings);
}

// --- the PATH axis: what the role vocabulary reaches, and what it structurally cannot ---
//
// The three tests below pin the two halves of the message's path disclosure together, because they
// only mean something as a pair: the first two pin the widening, the third pins the CEILING that
// widening does not move. Measured across the dogfood corpus's ten backend trees whose language this
// rule admits, six are reached and four are not. Recount:
//
//   for t in be-aspnet be-django be-express be-fastapi be-fastapi-fs be-gin be-nest be-spring \
//            be-spring-jwt spring-petclinic; do
//     n=$(rg --no-ignore --files "corpus/oss/$t" | tr '\' '/' | sed "s|^corpus/oss/$t/||" \
//         | grep -icE '(^|/)(api|server|backend|be|routes|controllers?|services?)/.*\.(ts|js|mjs|cjs|py|go|java|cs)$')
//     printf '%-18s %s\n' "$t" "$n"
//   done

#[test]
fn a_singular_controller_directory_fires_the_same_as_the_plural() {
    // The widening, and the reason it is not "more words": `controller/` names the SAME role the
    // pattern already claimed as `controllers/`. Spring's convention is the singular, and in the
    // dogfood corpus the singular is the spelling that actually occurs (2 files) while the shipped
    // plural occurs in none — the vocabulary was Express-shaped, so it missed its own concept.
    let dir = TempDir::new("zzop-hygiene");
    dir.write(
        "src/main/java/com/example/controller/UserController.java",
        "class UserController {\n  void handle() {\n    System.out.println(\"hit\");\n  }\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "console-in-be");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_eq!(h[0].line, 3);
}

#[test]
fn a_singular_service_directory_fires_the_same_as_the_plural() {
    let dir = TempDir::new("zzop-hygiene");
    dir.write(
        "src/main/java/com/example/service/UserService.java",
        "class UserService {\n  void run() {\n    System.err.println(\"hit\");\n  }\n}\n",
    );
    let out = scan(&dir);
    assert_eq!(hits(&out, "console-in-be").len(), 1, "{:?}", out.findings);
}

#[test]
fn a_domain_named_backend_directory_is_silent_and_that_is_the_disclosed_ceiling() {
    // NOT a bug to fix by adding words. This is the limit the message discloses: the rule recognizes
    // directories named for a ROLE, and the majority of real backends name theirs for their DOMAIN.
    // `articles/` here stands for `be-django`'s and `be-gin`'s layout; `be-aspnet` (`Articles/`,
    // `Features/`) and `be-nest` (`user/`, plus the role in the FILENAME as `*.controller.ts`) are the
    // other two misses. Those names are the application's own nouns and are unbounded, so no
    // vocabulary closes this — which is exactly why the message says silence here is absence of
    // evidence and never a clean bill.
    //
    // If this test ever goes red because someone taught the rule a domain noun, that is the signal to
    // re-read the disclosure rather than to delete this test: the honest fix for this class is a
    // different axis (a backend SIGNAL — a route registration, a server bind — not a path), and that
    // is a design decision, not a widening.
    let dir = TempDir::new("zzop-hygiene");
    dir.write("src/articles/handler.ts", "console.log(\"hit\");\n");
    dir.write("src/user/article.service.ts", "console.log(\"hit\");\n");
    let out = scan(&dir);
    assert!(hits(&out, "console-in-be").is_empty(), "{:?}", out.findings);
}
