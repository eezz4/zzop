//! NESTED reply-field shape pin: the KEY SET of a nested object in a real reply, against every
//! hand-written sentence that spells that shape.
//!
//! # The gap this closes
//! `crates/engine/tests/rule_contracts/surface_parity.rs` registers every TOP-LEVEL field of the
//! analyze/cross views and checks that the delivery surface carries it. One level down it sees
//! nothing, and one level down is where the prose actually lives: a tool description that says
//! `packsLoaded` is `{id, rules, source, filesInScope?, zeroAdmissionRules?, didNotRun?}` names a real, carried,
//! registered top-level field and is still wrong the day the entry grows a sixth key. Measured
//! 2026-08-20: `packsLoaded[].ruleIds` shipped and became the input the CLI's `--rule` refusal is
//! decided from, `coverageGaps`/`unreadExtensions` rows grew `kind`, and `architecture` grew
//! `topRecommendationMeaning`, while `cargo test -p zzop-mcp -p zzop-cli-bin` stayed GREEN with three
//! MCP tool descriptions and two site pages describing the older shapes.
//!
//! # What is derived and what is not
//! * The KEY SETS are derived — they come out of a real `analyze` run over a fixture this test writes,
//!   never from a list spelled here. A key added to a view type appears in the reply and this guard
//!   demands it in the prose on the next run.
//! * Which key is CONDITIONAL is derived too: a key present in every array element is required of the
//!   prose bare, a key present in some elements only may (and should) be spelled with a trailing `?`.
//!   The fixture is built so `packsLoaded` produces both variants, which is what makes that split
//!   measured rather than assumed.
//! * The PROSE SITES are derived: every non-test file under the roots below that mentions the field's
//!   own wire name is read. A new page describing `packsLoaded` is covered the day it lands, in the MCP
//!   tool schemas, on the site, or in docs/ alike.
//! * The AUDITED PATHS are a hand list, and that is the one thing here that can go stale silently.
//!   It cannot be derived from the reply, because most nested objects in a reply have no prose at all
//!   and demanding one for each would be a much larger decision than this guard. What IS checked in
//!   the other direction: every audited path must still be PRESENT and non-empty in the reply, so a
//!   removed or renamed field turns this row red instead of leaving it quietly vouching for nothing.
//!
//! # What this deliberately does NOT cover, and why
//! * **The `coverage` lane's own nested fields** — `unreadExtensions` (`{ext, kind, sharePct}`),
//!   `ioChannels`, `extensions`. Not an oversight and not a judgment that they are safe: this guard
//!   reads ONE reply, the `analyze` one, and those fields ride the separate `queryCoverage` reply that
//!   no MCP tool serves. Adding them means running a second lane here (`zzop_summary::coverage_summary`)
//!   and giving each an AUDITED row; their wire side is already pinned by
//!   `crates/facade/src/query_coverage/tests.rs`, so what is missing is the prose half alone.
//! * **`topRecommendation`'s own inner keys** (`{id, severity, topItem}`). A fixture small enough to
//!   run inside a unit test never clears the recommendation threshold, so the reply carries `null`
//!   there and there is no key set to derive. Deriving it would need a real repository-sized tree.
//! * **Test files.** `*/tests.rs` and anything under a `tests/` directory are skipped: a test's braces
//!   are code, not a description of the wire for a reader.
//! * **`README.md` and `CHANGELOG.md`.** Both spell these shapes and neither is a root here.
//!   `CHANGELOG.md` is excluded on purpose and must stay excluded: a released row records what a
//!   release SAID, and rewriting it to today's wire would destroy the only record this repo keeps of
//!   what changed. `README.md` is simply not wired up yet — one file, and the reason to be careful is
//!   that its abridged sample is deliberately partial.
//!
//! # Why the fixture is written INSIDE the repository
//! `architecture` is absent (not null) when git signals did not run, so a tree in the system temp
//! directory produces no `architecture` at all and this guard would vouch for nothing there. The
//! fixture therefore lives under `<repo>/target/` — gitignored, but inside the git working tree, which
//! is what makes `gitWindow` non-null. The floor below asserts that outcome rather than assuming it.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;

/// Directories whose files are read as PROSE describing the wire — hand-written English (or {ko, en}
/// pairs) that no compiler checks. Every file under them is read; nothing here lists pages.
const PROSE_ROOTS: [&str; 3] = ["packages/mcp/src", "site-src", "docs"];

/// How far after a mention of the field's own wire name a shape literal still counts as a claim about
/// that field. Generous, because `packages/mcp/src/tools/definitions.rs` holds each tool description on
/// ONE line and the sentence that names a field and the sentence that spells it are often paragraphs
/// apart; false association is prevented by [`OVERLAP_FLOOR`] instead of by a tight window.
const WINDOW: usize = 900;

/// A brace list found near a field's name is treated as a claim about THAT field only when it shares
/// at least this many keys with the field's real key set. Without it, a `{disabled, severityRemapped,
/// only}` sitting two sentences after the word `packsLoaded` would be judged as `packsLoaded`'s shape.
/// The cost is stated rather than hidden: a literal that drifted in EVERY key is not associated with
/// anything and passes unseen. That is the failure mode a rename produces, and `surface_parity.rs`
/// catches a renamed top-level field on its own.
const OVERLAP_FLOOR: usize = 2;

/// One nested object whose shape is spelled somewhere in prose.
struct Audited {
    /// JSON pointer into the analyze reply.
    pointer: &'static str,
    /// The wire name prose uses for it — also the anchor a describing sentence is found by.
    anchor: &'static str,
}

const AUDITED: &[Audited] = &[
    Audited {
        pointer: "/packsLoaded",
        anchor: "packsLoaded",
    },
    Audited {
        pointer: "/coverageGaps",
        anchor: "coverageGaps",
    },
    Audited {
        pointer: "/coverageGaps/extensions",
        anchor: "coverageGaps",
    },
    Audited {
        pointer: "/architecture",
        anchor: "architecture",
    },
];

// ---------------------------------------------------------------------------------------------
// The fixture, and the real run over it
// ---------------------------------------------------------------------------------------------

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("the repo root is two directories above packages/mcp")
}

struct Fixture(PathBuf);

impl Fixture {
    /// A tree shaped to light up every audited channel at once:
    /// * `.ts` files with resolvable imports, so the dependency graph is non-trivial and
    ///   `packsLoaded` reports non-zero `filesInScope` for the TypeScript-targeting packs;
    /// * `.vue` files no structural parser reads, so some pack's rules admit zero files and
    ///   `zeroAdmissionRules` appears on SOME entries but not all — which is what lets this test
    ///   derive, rather than assume, that the key is conditional;
    /// * `.json` data files above the principal floor, so `coverageGaps.extensions` is non-empty.
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = repo_root().join("target").join(format!(
            "nested-reply-shape-prose-{}-{n}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("src")).expect("fixture src");
        fs::create_dir_all(dir.join("data")).expect("fixture data");
        let f = Fixture(dir);
        f.write(
            zzop_config::DEFAULT_CONFIG_FILENAME,
            zzop_config::template::CONFIG_TEMPLATE_JSONC,
        );
        for i in 0..10 {
            f.write(
                &format!("src/f{i}.ts"),
                &format!("import {{ a }} from \"./m{i}\";\nexport const x{i} = a;\n"),
            );
            f.write(&format!("src/m{i}.ts"), &format!("export const a = {i};\n"));
            f.write(
                &format!("src/c{i}.vue"),
                &format!("<template><div>{i}</div></template>\n"),
            );
        }
        for i in 0..8 {
            // Wide enough to clear the principal-filetype LINE share as well as the file share.
            let body: String = (0..40)
                .map(|k| format!("  \"k{k}\": \"v{k}\",\n"))
                .collect();
            f.write(
                &format!("data/d{i}.json"),
                &format!("{{\n{body}  \"last\": {i}\n}}\n"),
            );
        }
        f
    }

    fn write(&self, rel: &str, content: &str) {
        let full = self.0.join(rel);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).expect("fixture parent");
        }
        fs::write(full, content).expect("fixture write");
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

// ---------------------------------------------------------------------------------------------
// Key sets, taken from the reply
// ---------------------------------------------------------------------------------------------

/// `(always, sometimes)` — keys carried by every element, and keys carried by only some. A plain
/// object is all-always. Returns `None` for anything with no keys to read, which the floor turns into
/// a failure rather than a silent pass.
fn key_sets(v: &Value) -> Option<(BTreeSet<String>, BTreeSet<String>)> {
    match v {
        Value::Object(m) => {
            let keys: BTreeSet<String> = m.keys().cloned().collect();
            (!keys.is_empty()).then(|| (keys, BTreeSet::new()))
        }
        Value::Array(items) => {
            let objs: Vec<&serde_json::Map<String, Value>> =
                items.iter().filter_map(Value::as_object).collect();
            if objs.is_empty() {
                return None;
            }
            let mut union: BTreeSet<String> = BTreeSet::new();
            for o in &objs {
                union.extend(o.keys().cloned());
            }
            let always: BTreeSet<String> = union
                .iter()
                .filter(|k| objs.iter().all(|o| o.contains_key(k.as_str())))
                .cloned()
                .collect();
            let sometimes: BTreeSet<String> = union.difference(&always).cloned().collect();
            (!union.is_empty()).then_some((always, sometimes))
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------------------------
// Shape literals, taken from the prose
// ---------------------------------------------------------------------------------------------

#[derive(Debug)]
struct ShapeLiteral {
    /// Byte offset of the opening brace, for the proximity test.
    start: usize,
    /// Keys spelled without a trailing `?`.
    required: BTreeSet<String>,
    /// Keys spelled with a trailing `?`.
    optional: BTreeSet<String>,
    /// `true` for a `{"k": v, ...}` illustration rather than a `{a, b}` shape declaration. An
    /// illustration may leave keys out; it may still not invent one.
    illustration: bool,
    /// An `…` / `...` element, the author's explicit "this list is not exhaustive".
    elided: bool,
    text: String,
}

impl ShapeLiteral {
    fn all(&self) -> BTreeSet<String> {
        self.required.union(&self.optional).cloned().collect()
    }
}

/// True when the bytes ending at `end` (exclusive) put the brace inside a code span — a backtick or a
/// `<code>` tag, allowing spaces between. Everything else is program syntax (`use a::{b, c}`, a Rust
/// struct literal, a `json!({...})` call) and is not a description of the wire.
fn opens_code_span(text: &str, end: usize) -> bool {
    let head = &text.as_bytes()[..end];
    let mut i = head.len();
    while i > 0 && (head[i - 1] == b' ' || head[i - 1] == b'\t') {
        i -= 1;
    }
    if i > 0 && head[i - 1] == b'`' {
        return true;
    }
    head[..i].ends_with(b"<code>")
}

/// Splits `body` on commas at brace/bracket depth 0.
fn split_top_level(body: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    for (i, c) in body.char_indices() {
        match c {
            '{' | '[' | '(' => depth += 1,
            '}' | ']' | ')' => depth -= 1,
            ',' if depth == 0 => {
                out.push(&body[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(&body[start..]);
    out
}

fn is_ident(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        && s.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
}

/// Reads every code-span brace list in `text`. A list qualifies only when EVERY element parses, all in
/// the same style — bare idents (`{a, b?}`) or quoted keys (`{"a": 1, "b": 2}`) — because a half-parsed
/// brace is prose with a brace in it, not a shape.
fn shape_literals(text: &str) -> Vec<ShapeLiteral> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    for start in 0..bytes.len() {
        if bytes[start] != b'{' || !opens_code_span(text, start) {
            continue;
        }
        let Some(end) = matching_brace(text, start) else {
            continue;
        };
        let body = &text[start + 1..end];
        let parts = split_top_level(body);
        if parts.len() < 2 {
            continue;
        }
        let mut required = BTreeSet::new();
        let mut optional = BTreeSet::new();
        let mut bare = 0usize;
        let mut quoted = 0usize;
        let mut elided = false;
        let mut ok = true;
        for raw in &parts {
            let p = raw.trim();
            if p == "…" || p == "..." {
                elided = true;
                continue;
            }
            if let Some(rest) = p.strip_prefix('"') {
                // `"name": value`
                let Some((name, tail)) = rest.split_once('"') else {
                    ok = false;
                    break;
                };
                if !is_ident(name) || !tail.trim_start().starts_with(':') {
                    ok = false;
                    break;
                }
                quoted += 1;
                required.insert(name.to_string());
                continue;
            }
            if let Some(name) = p.strip_suffix('?') {
                if !is_ident(name) {
                    ok = false;
                    break;
                }
                bare += 1;
                optional.insert(name.to_string());
                continue;
            }
            if !is_ident(p) {
                ok = false;
                break;
            }
            bare += 1;
            required.insert(p.to_string());
        }
        if !ok || (bare > 0 && quoted > 0) || (bare + quoted) < 2 {
            continue;
        }
        out.push(ShapeLiteral {
            start,
            required,
            optional,
            illustration: quoted > 0,
            elided,
            text: text[start..=end].to_string(),
        });
    }
    out
}

fn matching_brace(text: &str, open: usize) -> Option<usize> {
    let mut depth = 0i32;
    for (offset, &byte) in text.as_bytes()[open..].iter().enumerate() {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(open + offset);
                }
            }
            // A brace list never spans a line in this repo's prose, and refusing to cross one keeps a
            // stray `{` from swallowing the rest of a file.
            b'\n' => return None,
            _ => {}
        }
    }
    None
}

/// Every non-test file under the prose roots, so the site set is derived rather than listed.
fn prose_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            if p.is_dir() {
                if name != "tests" && name != "node_modules" {
                    stack.push(p);
                }
            } else if name != "tests.rs" {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

// ---------------------------------------------------------------------------------------------
// The check
// ---------------------------------------------------------------------------------------------

#[test]
fn every_prose_spelling_of_a_nested_reply_shape_matches_the_wire() {
    let root = repo_root();
    let fixture = Fixture::new();
    let filters = zzop_summary::FindingFilters::new(None, None, None)
        .expect("the no-filter view always constructs");
    let out = zzop_summary::analyze_summary(Some(&fixture.0.display().to_string()), None, &filters)
        .expect("analyzing this test's own fixture must succeed");
    let reply: Value = serde_json::from_str(&out).expect("the reply is JSON");

    // ---- floor: the run has to have exercised the channels before its key sets mean anything ----
    assert!(
        reply["fileCount"].as_u64().unwrap_or(0) > 0,
        "the fixture tree analyzed ZERO files, so every key set below would be empty and this guard \
         would pass by vouching for nothing. The most likely cause is that the walker started \
         honoring the repository's own `/target` .gitignore entry for a tree nested inside it — move \
         the fixture somewhere still inside the git working tree but not ignored. Reply: {reply}"
    );
    assert!(
        !reply["gitWindow"].is_null(),
        "`gitWindow` is null, so git signals did not run and the reply carries no `architecture` \
         object at all — one of the four shapes this guard exists to pin would silently drop out. The \
         fixture lives under <repo>/target/ precisely so it sits inside a git working tree; check \
         that `git` is on PATH and that this checkout is a git repository. Reply: {reply}"
    );

    let mut checked_claims = 0usize;

    for audited in AUDITED {
        let node = reply.pointer(audited.pointer).unwrap_or_else(|| {
            panic!(
                "the reply carries nothing at `{}`. Either the field was renamed or removed — in \
                 which case delete or repoint this row in AUDITED, and fix the prose that still \
                 names it — or the fixture stopped producing it. Reply: {reply}",
                audited.pointer
            )
        });
        let (always, sometimes) = key_sets(node).unwrap_or_else(|| {
            panic!(
            "`{}` is present but carries no keys ({node}), so there is no shape to pin and every \
             prose check below would pass vacuously. Extend the fixture in this file until this \
             channel is non-empty again.",
            audited.pointer
        )
        });
        let wire: BTreeSet<String> = always.union(&sometimes).cloned().collect();
        let wire_spelling = format!(
            "{{{}}}",
            wire.iter()
                .map(|k| if sometimes.contains(k) {
                    format!("{k}?")
                } else {
                    k.clone()
                })
                .collect::<Vec<_>>()
                .join(", ")
        );

        let mut claims_for_this_field = 0usize;

        for rel_root in PROSE_ROOTS {
            for file in prose_files(&root.join(rel_root)) {
                let Ok(text) = fs::read_to_string(&file) else {
                    continue;
                };
                if !text.contains(audited.anchor) {
                    continue;
                }
                let literals = shape_literals(&text);
                if literals.is_empty() {
                    continue;
                }
                let anchors: Vec<usize> =
                    text.match_indices(audited.anchor).map(|(i, _)| i).collect();
                let shown = file.strip_prefix(&root).unwrap_or(&file).display();
                let regenerate = if file.starts_with(root.join("site-src")) {
                    "\n  Then regenerate the published pages, or scripts/check-site-generated.sh \
                     goes red on the same edit:  node scripts/gen-site.mjs"
                } else {
                    ""
                };

                for lit in &literals {
                    let near = anchors
                        .iter()
                        .any(|a| lit.start > *a && lit.start - *a <= WINDOW);
                    if !near {
                        continue;
                    }
                    let lit_all = lit.all();
                    if lit_all.intersection(&wire).count() < OVERLAP_FLOOR {
                        continue;
                    }
                    claims_for_this_field += 1;
                    checked_claims += 1;

                    // Only keys the prose spells as REQUIRED can be judged invented. A `?`-marked key
                    // absent from this run is the state a `?` exists to describe, and this fixture is
                    // one tree: demanding that every optional key show up in it would make the guard
                    // red the day a fixture stops producing one, which is a false red about the
                    // fixture wearing the costume of a documentation defect. The cost, stated rather
                    // than hidden: an optional field REMOVED from the wire keeps its `?` entry in the
                    // prose unnoticed. A rename is still caught, on the missing-key side below.
                    let invented: Vec<&String> = lit.required.difference(&wire).collect();
                    assert!(
                        invented.is_empty(),
                        "{shown} describes `{}` with key(s) the wire does not carry: {invented:?}\n  \
                         it says:   {}\n  the wire: {wire_spelling}\n  Fix: rewrite that literal to \
                         exactly `{wire_spelling}`.{regenerate}",
                        audited.pointer,
                        lit.text
                    );

                    if !(lit.illustration && lit.elided) {
                        // An illustration may leave a CONDITIONAL key out — showing one would
                        // misrepresent the common reply — but not an always-present one.
                        let missing: Vec<&String> = if lit.illustration {
                            always.difference(&lit_all).collect()
                        } else {
                            // Both halves are demanded of a shape DECLARATION: an always-present key
                            // (the `ruleIds` class) and a conditional one (a consumer that never sees
                            // it in a sample reply has to learn it exists from somewhere).
                            wire.difference(&lit_all).collect()
                        };
                        let fix = if lit.illustration {
                            format!(
                                "add {missing:?} to that example with a representative value, or add \
                                 a `…` element to it to declare the example non-exhaustive (this is a \
                                 `\"key\": value` illustration, so it is NOT rewritten to a bare shape \
                                 list). Every always-present key: {always:?}"
                            )
                        } else {
                            format!("rewrite that literal to exactly `{wire_spelling}`")
                        };
                        assert!(
                            missing.is_empty(),
                            "{shown} describes `{}` without key(s) the wire carries: {missing:?}\n  \
                             it says:   {}\n  the wire: {wire_spelling}\n  Fix: {fix}.{regenerate}",
                            audited.pointer,
                            lit.text
                        );
                    }

                    let overstated: Vec<&String> = lit.required.difference(&always).collect();
                    assert!(
                        lit.illustration || overstated.is_empty(),
                        "{shown} spells key(s) of `{}` as always-present that this run shows are \
                         CONDITIONAL: {overstated:?}\n  it says:   {}\n  the wire: {wire_spelling}\n  \
                         Fix: rewrite that literal to exactly `{wire_spelling}` — the `?` is what \
                         tells a consumer an absent key is not an error.{regenerate}",
                        audited.pointer,
                        lit.text
                    );
                }
            }
        }

        assert!(
            claims_for_this_field > 0,
            "no file under {PROSE_ROOTS:?} spells the shape of `{}` ({wire_spelling}) within \
             {WINDOW} bytes of a mention of `{}`. This guard then vouches for nothing on that field. \
             Either write the shape into the description that names it — the MCP tool descriptions in \
             packages/mcp/src/tools/definitions.rs are the surface an agent actually reads — or, if \
             the field genuinely has no prose anywhere, drop its row from AUDITED and record the \
             reason in this file's header beside the other declared exclusions.",
            audited.pointer,
            audited.anchor
        );
    }

    // Belt to the per-field floor's braces: the loop above can only be vacuous if AUDITED is empty.
    assert!(
        checked_claims >= AUDITED.len(),
        "this run checked {checked_claims} prose claim(s) against {} audited field(s) — fewer claims \
         than fields means a field slipped through the per-field floor.",
        AUDITED.len()
    );
}
