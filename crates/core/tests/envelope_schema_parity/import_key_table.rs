//! `ImportBinding`'s per-language KEY table, held against the front ends themselves — and the two
//! shipped documents that publish the same contract to adapter authors.
//!
//! ## Why the table exists
//! `crates/core/src/ir/imports.rs` said "Keyed by localName", unconditionally, until 2026-08-14. C#
//! does not do that: a plain `using A.B;` is keyed by the FULL SPECIFIER, because `using A.Models;`
//! and `using B.Models;` are both legal in one file and a last-segment key would collide in the
//! `BTreeMap` and silently drop one witnessed namespace. That reasoning lived only in the deviating
//! parser's own module doc, while the file that OWNS the contract — the one an external adapter author
//! copies from — kept making the universal claim. Five front ends also mint synthetic keys for imports
//! that bind no name at all, and two mint no bindings whatsoever.
//!
//! ## What this checks, in both directions
//! Writing an inventory down creates the defect class this repo spends most of its guards on: prose
//! stating a fact, with nothing holding it. Three derived axes, each bidirectional, because each
//! direction fails silently on its own — a table that under-promises makes an adapter author avoid a
//! key that works, and one that over-promises makes them target a key nothing ever emits.
//!
//! 1. **Coverage** — the subject set is every `parser/parser-*` crate ON DISK, never a list here. A
//!    ninth front end joins this check by existing (it fails until it has a row).
//! 2. **Bindings at all** — a crate that constructs an `ImportBinding` in its shipped sources must have
//!    a non-dashed row, and a crate that constructs none must be dashed. That is what keeps the Prisma
//!    and SQL rows honest the day either grows an import channel.
//! 3. **Synthetic keys** — every double-underscore-delimited string literal in a shipped file that
//!    constructs an `ImportBinding` must be claimed by that language's row, and every key the row
//!    claims must be matched by one of those literals.
//!
//! ## What the needle does NOT see, and why that is written here
//! Axis 3's subject set is DERIVED (files constructing an `ImportBinding`), but within those files the
//! needle is a string literal shaped `"__...__"`, compared as a SET after every `{...}` placeholder
//! collapses to `*`. Two consequences, both deliberate:
//!
//! - It cannot see the ORDINARY key rule at all — "the rightmost dotted segment" is not a literal any
//!   scan can read, so column 2 of the table is prose this test does not judge. That column is exactly
//!   where the C# deviation lives. The honest statement of this guard's reach: it holds the SHAPE of
//!   the table (who has rows, who binds at all, which non-name keys exist), and the per-language key
//!   RULE is held by the parsers' own unit tests plus review. A green here is not "column 2 checked".
//! - A key whose distinguishing part is INTERPOLATED is seen once, not once per value. Go builds both
//!   its non-name keys from `format!("__{label}_import_{}__")`, so the machine sees the single shape
//!   `__*_import_*__` and the two labels are prose in the cell beside it. The table therefore spells
//!   that row the way the source does. This is the second design: the first accepted a claimed key if
//!   any source pattern wildcard-matched it, and the invalidation drill caught it going GREEN with
//!   `__blank_import_{N}__` deleted from the table — a matcher loose enough to absorb the placeholder
//!   was loose enough to absorb a deletion. Set equality gives that direction back, at the price of
//!   this one named blind spot: a THIRD Go label would not turn it red, while any new key SPELLING in
//!   any front end would.
//!
//! The extraction is textual, like `catalog_sync.rs`'s `SourceSymbolKind` collapse pin, for the
//! structural reason: `zzop-core` does not — and must not — depend on the parsers it defines the IR for.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

/// A dash cell (em dash) means "this front end has no such key at all" — the same vocabulary
/// `kinds.rs`'s collapse table uses for an unaskable question.
const DASH: &str = "—";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// One row of the table: does this front end bind anything, and which keys does it claim are not
/// local names.
struct Row {
    binds: bool,
    synthetic: BTreeSet<String>,
}

/// Table label -> crate directory. The one hand-written mapping here, and it is checked: a crate with
/// no label fails below rather than being skipped.
fn label_of(crate_name: &str) -> Option<&'static str> {
    Some(match crate_name {
        "parser-typescript" => "TypeScript",
        "parser-python-3" => "Python",
        "parser-java-21" => "Java",
        "parser-csharp" => "C#",
        "parser-rust" => "Rust",
        "parser-go" => "Go",
        "parser-prisma" => "Prisma",
        "parser-sql" => "SQL",
        _ => return None,
    })
}

/// `{...}` (a `format!` placeholder, or the `{N}` the table writes) -> `*`, so the two sides are
/// comparable regardless of which one spells the counter.
fn normalize(raw: &str) -> String {
    let mut out = String::new();
    let mut depth = 0usize;
    for ch in raw.chars() {
        match ch {
            '{' => {
                if depth == 0 {
                    out.push('*');
                }
                depth += 1;
            }
            '}' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(ch),
            _ => {}
        }
    }
    out
}

/// Every backtick-quoted span of a table cell that looks like a synthetic key.
fn claimed_keys(cell: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for span in cell.split('`').skip(1).step_by(2) {
        let key = normalize(span.trim());
        if key.starts_with("__") && key.ends_with("__") && key.len() > 4 {
            out.insert(key);
        }
    }
    out
}

/// The table in `crates/core/src/ir/imports.rs`, parsed into rows.
fn claimed_table() -> BTreeMap<String, Row> {
    let src = fs::read_to_string(repo_root().join("crates/core/src/ir/imports.rs"))
        .expect("crates/core/src/ir/imports.rs is readable");
    let mut rows = BTreeMap::new();
    for line in src.lines() {
        let Some(row) = line.trim().strip_prefix("/// |") else {
            continue;
        };
        let cells: Vec<&str> = row.split('|').map(str::trim).collect();
        // Requires a label plus exactly the two content cells, which skips the separator row and any
        // other pipe-bearing prose; the header row is skipped by its own label.
        if cells.len() < 3 || cells[0].is_empty() || cells[0].starts_with("---") {
            continue;
        }
        if cells[0] == "Front end" {
            continue;
        }
        rows.insert(
            cells[0].to_string(),
            Row {
                binds: cells[1] != DASH,
                synthetic: claimed_keys(cells[2]),
            },
        );
    }
    rows
}

/// A shipped (non-test) source file — the same exclusion `catalog_sync.rs` uses, for the same reason:
/// a test asserting a key is not the parser emitting one.
fn is_shipped(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    name.ends_with(".rs")
        && name != "tests.rs"
        && name != "test_util.rs"
        && !name.ends_with("_tests.rs")
}

/// Walks a parser crate's `src`, returning (constructs an `ImportBinding`, synthetic key literals).
/// The literal scan is confined to the files that CONSTRUCT a binding — derived, not listed — which is
/// what keeps unrelated dunder literals (`__all__`, `__tablename__`, `__init__.py`, all of which live
/// in other modules of the same crates) out of the comparison.
fn emitted(dir: &Path, binds: &mut bool, keys: &mut BTreeSet<String>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            emitted(&path, binds, keys);
            continue;
        }
        if !is_shipped(&path) {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        if !text.contains("ImportBinding {") {
            continue;
        }
        *binds = true;
        for (at, _) in text.match_indices("\"__") {
            let after = &text[at + 1..];
            let Some(end) = after.find('"') else {
                continue;
            };
            let literal = normalize(&after[..end]);
            if literal.starts_with("__") && literal.ends_with("__") && literal.len() > 4 {
                keys.insert(literal);
            }
        }
    }
}

#[test]
fn the_import_key_table_matches_what_each_parser_emits() {
    let claimed = claimed_table();
    assert!(
        claimed.len() >= 8,
        "parsed {} row(s) out of the key table in crates/core/src/ir/imports.rs — the table's markdown \
         shape changed and this pin would judge almost nothing. Re-point the parse in the same commit \
         as the reformat.",
        claimed.len()
    );

    let parser_dir = repo_root().join("parser");
    let mut unlabelled = Vec::new();
    let mut checked = 0usize;
    let mut binding_crates = 0usize;
    let mut synthetic_seen = 0usize;
    for entry in fs::read_dir(&parser_dir)
        .expect("parser/ is readable")
        .flatten()
    {
        let crate_name = entry.file_name().to_string_lossy().to_string();
        if !crate_name.starts_with("parser-") {
            continue;
        }
        let Some(label) = label_of(&crate_name) else {
            unlabelled.push(crate_name.clone());
            continue;
        };
        let Some(row) = claimed.get(label) else {
            unlabelled.push(crate_name.clone());
            continue;
        };

        let mut binds = false;
        let mut keys = BTreeSet::new();
        emitted(&entry.path().join("src"), &mut binds, &mut keys);

        assert_eq!(
            row.binds, binds,
            "the key table in crates/core/src/ir/imports.rs says {label} {}, but {crate_name}'s \
             shipped sources {} construct an ImportBinding. A dashed row promises a consumer that \
             asking this tree for `imports` is an unanswerable question, not an empty answer — the two \
             are different facts and only one of them is true here.",
            if row.binds {
                "binds imports"
            } else {
                "binds none"
            },
            if binds { "DO" } else { "do NOT" }
        );

        let over_promised: Vec<&String> = row.synthetic.difference(&keys).collect();
        assert!(
            over_promised.is_empty(),
            "the key table claims {label} emits the synthetic key(s) {over_promised:?}, which no \
             shipped source of {crate_name} that builds an ImportBinding spells (it spells {keys:?}). \
             An adapter author reads that column to know which keys are NOT theirs to collide with; a \
             key nothing emits sends them around an obstacle that is not there."
        );
        let under_promised: Vec<&String> = keys.difference(&row.synthetic).collect();
        assert!(
            under_promised.is_empty(),
            "{crate_name} builds the synthetic ImportMap key(s) {under_promised:?}, which the key \
             table in crates/core/src/ir/imports.rs does not list for {label} (it lists {:?}). Every \
             key that is not a local name has to be in that column — an unlisted one is precisely the \
             case where a consumer's \"keyed by local name\" assumption breaks with no error.",
            row.synthetic
        );
        synthetic_seen += keys.len();

        checked += 1;
        if binds {
            binding_crates += 1;
        }
    }

    assert!(
        unlabelled.is_empty(),
        "these parser crates have no row in the key table (or no label in this test): {unlabelled:?}. \
         A front end with no row is one whose ImportMap key vocabulary nobody has written down."
    );
    // Non-vacuity, three floors: a zero anywhere here reads exactly like a clean run.
    assert!(
        checked >= 8,
        "only {checked} parser crate(s) were compared — the crate walk narrowed and this pin would \
         vouch for a table it barely read"
    );
    assert!(
        binding_crates >= 6,
        "only {binding_crates} parser crate(s) were found to construct an ImportBinding at all, down \
         from 6. Either the construction needle stopped matching (in which case every row's `binds` \
         axis is now trivially comparing false to false) or front ends lost their import channel"
    );
    assert!(
        synthetic_seen >= 6,
        "only {synthetic_seen} synthetic key literal(s) were extracted, down from 6 — the literal \
         needle went blind and the synthetic-key axis is comparing two empty sets"
    );
}

/// The same claim ships to adapter authors twice more, and both copies are BAKED INTO THE BINARY by
/// `crates/summary/src/contracts.rs` (`docs/NORMALIZED_AST.md`, `docs/adapters/envelope.schema.json`) —
/// which is why a pointer, not a restatement, is what each is allowed to carry. This repo has already
/// paid for the alternative: one claim corrected in two documents by two independent edits, each
/// leaving sites the other had not touched.
///
/// Like the span-contract guard next door, this checks the LINK and not the prose — asserting on
/// wording would go red on every honest rewrite, and the link is the load-bearing part anyway: a
/// document carrying the pointer is one whose next editor is told where the truth lives before they
/// restate it.
#[test]
fn every_doc_naming_the_import_map_key_points_at_the_table_that_owns_it() {
    /// Both spellings: the wire one the schema and the AST document use, and the prose one the
    /// adapter guides use for the same key.
    const KEY_WORDS: [&str; 2] = ["localName", "local name"];
    /// Either form of the pointer — the path a reader clicks, or the phrase they search for.
    const OWNER_REFS: [&str; 2] = ["core/src/ir/imports.rs", "per-language key table"];

    fn documents(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(dir).unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()))
        {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                documents(&path, out);
            } else if path.extension().is_some_and(|e| e == "md" || e == "json") {
                out.push(path);
            }
        }
    }

    let docs = repo_root()
        .join("docs")
        .canonicalize()
        .expect("docs/ must exist relative to crates/core");
    let mut files = Vec::new();
    documents(&docs, &mut files);
    files.sort();

    let mut mentioning = 0usize;
    let mut offenders = Vec::new();
    for file in &files {
        let text = fs::read_to_string(file)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", file.display()));
        if !KEY_WORDS.iter().any(|w| text.contains(w)) {
            continue;
        }
        mentioning += 1;
        if !OWNER_REFS.iter().any(|r| text.contains(r)) {
            offenders.push(
                file.strip_prefix(&docs)
                    .unwrap_or(file)
                    .display()
                    .to_string(),
            );
        }
    }

    assert!(
        mentioning >= 4,
        "only {mentioning} document(s) under docs/ name the import-map key, down from 4. Either the \
         wording moved — in which case update KEY_WORDS, because this guard is now watching a phrase \
         nothing uses — or documents were removed and the coverage this test claims is gone"
    );
    assert!(
        offenders.is_empty(),
        "these docs/ document(s) describe the import-map key without pointing at the table that owns \
         it ({OWNER_REFS:?}): {offenders:?}. The key is universal for most front ends and NOT for C#, \
         so an unqualified restatement is a claim that is false on one of the eight — and the reader \
         it misleads is an adapter author whose displacing binding silently lands as a sibling."
    );
}
