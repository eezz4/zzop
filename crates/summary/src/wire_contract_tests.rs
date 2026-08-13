//! Pins `site/reference.html`'s `AnalyzeRequest` field table to the field list the wire type actually
//! accepts, DERIVED BY RUNNING THE DESERIALIZER rather than by reading its source.
//!
//! Same shape as `graph/cosmograph/tests.rs`'s published-schema pin, one layer up: that one runs the
//! emitter and compares the emitted key set against the documents that publish it; this one runs the
//! *de*serializer and compares the accepted key set against the document that publishes it. The drift
//! it closes was measured by hand on 2026-08-08 — the page's table carried 19 rows for a 21-field
//! struct, so two knobs a caller may legally send were undocumented while the page reads as a closed
//! list.
//!
//! WHY NOT `serde_json::to_value(&request)`, the obvious answer: `AnalyzeRequest` derives
//! `Deserialize` only (it is an input shape — nothing ever serializes one), and even if it did, an
//! `Option` field left `None` emits no key, so a serialized default instance would publish a SUBSET of
//! the contract and the pin would quietly assert less every time an optional field was added. That is
//! the bottomless derivation this module exists to avoid: the truth side here is the static field-name
//! array that serde's derive hands to `Deserializer::deserialize_struct`, which lists every field
//! regardless of whether any value is present, so there is no instance to under-fill.

use std::collections::BTreeSet;
use std::fmt;

use serde::de::{self, DeserializeOwned, Visitor};
use serde::Deserializer;
use zzop_facade::AnalyzeRequest;

/// Not a failure: the probe below stops the derive the instant it has the field list, and an error is
/// the only way a `Deserializer` can decline to produce a value.
#[derive(Debug)]
struct Captured;

impl fmt::Display for Captured {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("field list captured; deserialization deliberately abandoned")
    }
}

impl std::error::Error for Captured {}

impl de::Error for Captured {
    fn custom<T: fmt::Display>(_msg: T) -> Self {
        Captured
    }
}

/// A `Deserializer` that produces nothing and records one thing: the struct name and the field-name
/// array serde's generated code passes to `deserialize_struct`.
struct FieldProbe<'a>(&'a mut Option<(&'static str, Vec<&'static str>)>);

impl<'de> Deserializer<'de> for FieldProbe<'_> {
    type Error = Captured;

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        name: &'static str,
        fields: &'static [&'static str],
        _visitor: V,
    ) -> Result<V::Value, Captured> {
        *self.0 = Some((name, fields.to_vec()));
        Err(Captured)
    }

    fn deserialize_any<V: Visitor<'de>>(self, _visitor: V) -> Result<V::Value, Captured> {
        Err(Captured)
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes byte_buf option
        unit unit_struct newtype_struct seq tuple tuple_struct map enum identifier ignored_any
    }
}

/// The wire field names of `T`, as the derive itself declares them (so `rename_all = "camelCase"` is
/// already applied, and an `Option` field that is usually absent is still listed).
fn wire_fields<T: DeserializeOwned>() -> (&'static str, BTreeSet<String>) {
    let mut captured = None;
    let _ = T::deserialize(FieldProbe(&mut captured));
    let (name, fields) = captured.expect(
        "the request type's `Deserialize` impl never reached `deserialize_struct` — a \
         `#[serde(flatten)]` field makes the derive call `deserialize_map` instead, and a flattened \
         shape has no static field list at all. This pin cannot enumerate such a type; it fails loudly \
         rather than comparing the empty set against the document and calling that agreement.",
    );
    assert!(
        !fields.is_empty(),
        "{name} reported an EMPTY field list — every field is `#[serde(skip)]`, or the derive changed \
         shape. An empty truth side would make any document trivially correct."
    );
    (name, fields.iter().map(|f| (*f).to_string()).collect())
}

/// Independent membership oracle, checking the direction the field-list capture cannot check itself.
///
/// `-1` is a value NO field of `AnalyzeRequest` can hold: every field is a string, a bool, a `usize`,
/// a sequence, a map, or a struct. So a request carrying `{"<name>": -1}` fails to deserialize exactly
/// when `<name>` is a real field, and succeeds when it is not (`deny_unknown_fields` is deliberately
/// off, so an unknown key is ignored). The negative control in the test below is what proves this
/// discriminates instead of always answering the same way.
fn is_real_field(name: &str) -> bool {
    let json = format!("{{\"root\":\"x\",\"{name}\":-1}}");
    serde_json::from_str::<AnalyzeRequest>(&json).is_err()
}

fn read_repo_file(rel: &str) -> String {
    // Same three lines as `graph/cosmograph/tests.rs`'s helper, deliberately not shared: hoisting a
    // path join into a crate-wide test module would put a second `mod` in `lib.rs` whose only content
    // is `../..`.
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// The container the page's `<h2>{ty}</h2>` field index lives in, sliced at its own closing tag.
///
/// Anchored on a PURPOSE-NAMED attribute the page carries for this pin (`data-wire-fields`, written
/// last in the opening tag) rather than on the styling class that happens to lay the index out. A
/// class is presentation and gets renamed by a redesign; this attribute exists only to say "the
/// wire-field roster is inside me", so the next redesign carries it along or fails here loudly.
/// It is a marker, not a second copy of the roster — every field name is still read from the one
/// place a reader sees it.
///
/// Delimited by counting nested `<div>`s rather than by scanning to the next section marker: the
/// index holds `<details>` folds whose bodies are divs, so "the first `</div>` after the opening
/// tag" would cut the roster short at the first folded row, and this pin would read a three-row
/// list and blame the struct.
fn field_index_body<'a>(after: &'a str, heading: &str) -> &'a str {
    const INDEX_ANCHOR: &str = "data-wire-fields>";
    let start = after.find(INDEX_ANCHOR).unwrap_or_else(|| {
        panic!("no `data-wire-fields` index container follows the `{heading}` heading")
    });
    let body_start = start + INDEX_ANCHOR.len();
    let tag_start = after[..start]
        .rfind('<')
        .expect("the marker cannot appear before the first tag on the page");
    assert!(
        after[tag_start..].starts_with("<div ") && !after[tag_start..start].contains('>'),
        "the `data-wire-fields` marker after `{heading}` is not the last attribute of a `<div>` \
         opening tag, so the nesting walk below would start mid-markup and slice the wrong region"
    );

    let mut depth = 1usize;
    let mut cursor = body_start;
    let end = loop {
        let next_open = after[cursor..].find("<div").map(|p| cursor + p);
        let next_close = after[cursor..].find("</div>").map(|p| cursor + p);
        match (next_open, next_close) {
            (_, None) => panic!("the field index under `{heading}` is unterminated"),
            (Some(open), Some(close)) if open < close => {
                depth += 1;
                cursor = open + "<div".len();
            }
            (_, Some(close)) => {
                depth -= 1;
                if depth == 0 {
                    break close;
                }
                cursor = close + "</div>".len();
            }
        }
    };
    &after[body_start..end]
}

/// The field names the page's `<h2>{ty}</h2>` index publishes in its name column.
///
/// Located by the TYPE NAME the deserializer reported, not by a hand-typed heading string: renaming
/// the Rust struct then fails this pin at the anchor with a clear reason, instead of leaving it
/// pointing at a heading that no longer describes anything.
///
/// The needle follows the markup, and the markup changed: these two tables stopped being `<table>`s
/// when the page split its index from its contract prose (the `AnalyzeRequest.vocabulary` cell had
/// reached 1,999 characters, so the widest cell was setting the layout for twenty other rows).
/// Every row is now a `.cmd-row` grid line whose name cell opens with a bare `<code>field</code>`,
/// and a row long enough to need its contract carries that contract in a `<details>` fold directly
/// underneath rather than in the cell.
///
/// The extracted count is cross-checked against the number of rows in the same container, so a row
/// whose name cell is not a bare `<code>name</code>` is a failure rather than a silent omission —
/// otherwise a reformatted row would shrink the document side and the equality below would blame
/// the struct. Band labels (`.cmd-band`) and the column header (`.cmd-index__head`) are not rows and
/// are not counted; the `class="cmd-row"` needle carries its closing quote so it cannot match the
/// `cmd-row__name` / `cmd-row__desc` cells inside a row.
fn documented_fields(page: &str, ty: &str) -> BTreeSet<String> {
    let heading = format!("<h2>{ty}</h2>");
    let occurrences = page.matches(&heading).count();
    assert_eq!(
        occurrences, 1,
        "site/reference.html must carry exactly one `{heading}` heading for this pin to anchor on"
    );
    let after = &page[page.find(&heading).expect("counted above") + heading.len()..];
    let body = field_index_body(after, &heading);

    let row =
        regex::Regex::new("<span class=\"cmd-row__name\"><code>([A-Za-z][A-Za-z0-9]*)</code>")
            .expect("static pattern");
    let names: BTreeSet<String> = row.captures_iter(body).map(|c| c[1].to_string()).collect();
    let rows = body.matches("class=\"cmd-row\"").count();
    assert_eq!(
        names.len(),
        rows,
        "the `{ty}` index has {rows} rows but only {} of them open with a bare \
         `<span class=\"cmd-row__name\"><code>field</code>` cell naming one field. Every row must, \
         or this pin silently reads a short list and blames the wrong side.",
        names.len()
    );
    names
}

/// The drift guard: `site/reference.html`'s `AnalyzeRequest` table names exactly the fields the wire
/// type accepts — no missing row (a knob a caller may legally send that the page hides) and no extra
/// row (a knob the page promises that the deserializer ignores).
///
/// Scope, stated honestly. This pins the SET OF FIELD NAMES and nothing else: a row's type cell and
/// its prose are not read, so a documented field can still carry a wrong type or a stale description.
/// Pinning those would mean pinning wording, which this repo does not do. What it does instead is make
/// the roster itself underivable by hand — which is the half that was measured wrong.
#[test]
fn reference_html_publishes_exactly_the_analyze_request_wire_fields() {
    let (ty, accepted) = wire_fields::<AnalyzeRequest>();
    let documented = documented_fields(&read_repo_file("site/reference.html"), ty);

    // Negative control for the oracle, run FIRST: a name that is no field at all must deserialize
    // fine. Without it, an `is_real_field` that answered `true` unconditionally (a probe value some
    // future field could actually hold, say) would read as unanimous confirmation below.
    assert!(
        !is_real_field("zzopNotAFieldControlProbe"),
        "the `-1` membership probe rejected a name that is not a field at all, so it no longer \
         discriminates real keys from unknown ones — every assertion it backs below is vacuous."
    );

    // Two independent derivations of `what this type accepts` must agree: the static field list the
    // derive declares, and what the deserializer actually does with each name.
    for name in &accepted {
        assert!(
            is_real_field(name),
            "{ty} declares `{name}` in its serde field list, but a request carrying it is accepted \
             with the key ignored — the two derivations disagree."
        );
    }
    for name in &documented {
        assert!(
            is_real_field(name),
            "site/reference.html's {ty} table documents a `{name}` field, but the deserializer ignores \
             that key: a caller who sends it gets silence, not the behavior the page promises."
        );
    }

    let missing: Vec<&String> = accepted.difference(&documented).collect();
    let extra: Vec<&String> = documented.difference(&accepted).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "site/reference.html's {ty} table and the wire type disagree. Accepted but undocumented (no \
         row): {missing:?}. Documented but not accepted (phantom row): {extra:?}. The page reads as a \
         closed list, so a missing row is a knob nobody can discover and a phantom row is a knob that \
         silently does nothing."
    );
}

/// The output half of the same drift guard — pinned TRANSITIVELY rather than by runtime enumeration.
///
/// The truth side cannot be reached the way the input half reaches it: `AnalyzeOutputView` derives
/// `Serialize` only (there is no `deserialize_struct` to intercept), it is `pub(crate)` in another
/// crate, and running the producer instead would put the `Option` trap back — two of its fields are
/// `skip_serializing_if`, so a single fixture publishes a subset and the pin would quietly shrink.
///
/// `docs/contracts/surface-parity.json` is already that roster, and it is already machine-checked
/// against the real reply from the other side: `analyze::tests`' shaped-reply test fails on any
/// top-level key that is neither a registry row nor a declared shaper invention, and the engine's
/// `rule_contracts::surface_parity` completeness test fails on a view field with no row. So pinning
/// the page to the registry chains the page to the wire without this crate enumerating anything.
///
/// Note the registry is one layer wider than the type the page's heading names: the reply root
/// flattens `AnalyzeOutputView` and adds the run-global `disclosure` sibling. That is why the anchor
/// here is a literal heading rather than a reported type name — there is no single Rust type whose
/// field list equals this table, which is exactly the fact the page now discloses in prose.
#[test]
fn reference_html_publishes_exactly_the_registry_s_output_fields() {
    let registry: serde_json::Value =
        serde_json::from_str(&read_repo_file("docs/contracts/surface-parity.json"))
            .expect("the surface-parity registry must be valid JSON");
    let rows = registry["analyzeOutputView"]
        .as_object()
        .expect("the registry must carry an `analyzeOutputView` object");
    let declared: BTreeSet<String> = rows
        .keys()
        .filter(|k| !k.starts_with('_'))
        .cloned()
        .collect();

    // Floor. A registry that stopped parsing into rows would make every set below empty, and empty
    // sets agree with each other — the failure would read as a pass.
    assert!(
        declared.len() > 10,
        "the registry yielded only {} output rows, which is too few to be the real reply — the \
         extraction broke, and an empty truth side would agree with an empty document side.",
        declared.len()
    );

    let documented = documented_fields(&read_repo_file("site/reference.html"), "AnalyzeOutputView");
    let missing: Vec<&String> = declared.difference(&documented).collect();
    let extra: Vec<&String> = documented.difference(&declared).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "site/reference.html's output table and docs/contracts/surface-parity.json disagree. In the \
         registry but with no row on the page: {missing:?}. On the page but in no registry row: \
         {extra:?}. The registry is checked against the actual reply, so the page is the wrong side."
    );
}
