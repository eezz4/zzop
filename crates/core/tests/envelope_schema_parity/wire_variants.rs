//! COMPILER-ENFORCED variant binding for every externally-tagged WIRE ENUM — see the target doc in
//! `main.rs`, "Beyond key-NAME presence" section.
//!
//! ## The hole this closes
//! `RouterMountEntry::MountRef` once shipped in the engine while `envelope.schema.json` did not know
//! the variant. Because the schema's enum wrapper is `additionalProperties: false` + `maxProperties:
//! 1`, an external adapter emitting exactly the shape zzop itself emits was schema-INVALID. Nothing
//! caught it: the variant reached this harness only through hand-written `sample_mount_*()`
//! constructors, and no sample means no coverage — a new variant with no sample was invisible.
//!
//! ## The mechanism (why the compiler, not a reviewer, enforces it)
//! Each wire enum gets a MIRROR unit enum declared through [`mirror_enum!`], which emits the enum AND
//! its `ALL` array from the SAME token list — the two cannot drift, so there is no "forgot to extend
//! the array" hole. Three matches, none with a `_` arm, then chain the work:
//!   1. `WireEnum::of` matches the REAL enum → adding a variant to `RouterMountEntry` (or
//!      `ProcedureRouterEntry`, or `EntityRef`) is a COMPILE ERROR here, in this file.
//!   2. Writing that arm requires naming a mirror variant, which must be added to the
//!      `mirror_enum!` list — and that immediately breaks `WireEnum::tag` and `WireEnum::sample`
//!      (both exhaustive over the mirror), forcing a wire tag AND a sample instance to exist.
//!   3. The new mirror variant is in `ALL` by construction, so [`assert_wire_enum_bound`] iterates it
//!      and FAILS until `definitions.<def>.properties` declares that tag.
//!
//! Only a schema edit ends the chain. The wire tag is re-derived BY HAND in `tag()` rather than read
//! back from serde, so a `#[serde(rename)]` on the Rust side is caught too, not just a new variant —
//! the same reasoning `enum_parity.rs`'s `source_symbol_kind_wire` records for value-set enums.
//!
//! ## The reverse direction
//! [`assert_wire_enum_bound`] also diffs schema→Rust: a wrapper property with no Rust variant fails,
//! naming it. That is what makes an emptied variant list loud instead of vacuously green, and it
//! follows the shape `main.rs`'s `schema_definitions_cover_exactly_the_expected_type_set` set.
//!
//! ## Downstream reuse — this is the ONE variant list
//! `samples.rs` builds `RouterMountFragment`/`ProcedureRouterFragment` entries and the per-file
//! `attributes` channel from `ALL`, and `required_nullable.rs` loops over `ALL`. So a new variant is
//! automatically walked by the key-set parity guard and probed for required-ness/nullability too —
//! no second hand-maintained list anywhere.

use std::collections::HashSet;

use serde_json::Value;

use zzop_core::{Attribute, EntityRef, ProcedureRouterEntry, RouterMountEntry};

use crate::{assert_parity, def, obj, props};

/// Declares a mirror unit enum plus its `ALL` array from one token list, so the array can never omit,
/// duplicate, or reorder a variant — the array is generated from the same tokens as the enum itself.
macro_rules! mirror_enum {
    ($(#[$meta:meta])* $name:ident { $($variant:ident),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Copy, Clone, PartialEq, Eq, Debug)]
        pub(crate) enum $name {
            $($variant,)+
        }

        impl $name {
            /// Every variant, generated from this enum's own declaration — see [`mirror_enum!`].
            const ALL: &'static [Self] = &[$(Self::$variant,)+];
        }
    };
}

/// One externally-tagged wire enum, bound variant-for-variant to one schema wrapper definition. Every
/// method is implemented by an EXHAUSTIVE match with no wildcard arm — see the module doc for how the
/// three of them chain into "a new variant cannot compile until the schema declares it".
pub(crate) trait WireEnum: Copy + Eq + std::fmt::Debug + 'static {
    /// The real serde type whose variants this mirrors.
    type Wire: serde::Serialize + serde::de::DeserializeOwned;

    /// The `definitions.<name>` entry that wraps this enum (`additionalProperties: false` +
    /// `maxProperties: 1`, one property per variant).
    const SCHEMA_DEF: &'static str;

    fn all() -> &'static [Self];

    /// The variant's wire tag, RE-DERIVED BY HAND (never read back from serde) so a `#[serde(rename)]`
    /// or a `rename_all` change is caught as drift instead of silently agreeing with itself.
    fn tag(self) -> &'static str;

    /// A sample instance of exactly this variant, fully populated in the same sense as `samples.rs`:
    /// every `Option` `Some` and every `skip_serializing_if`-gated collection non-empty, so the
    /// serialized form actually carries every field the schema documents.
    fn sample(self) -> Self::Wire;

    /// Which variant a real value is — the exhaustive match over the REAL enum, i.e. the arm that
    /// stops compiling when someone adds a variant upstream.
    fn of(wire: &Self::Wire) -> Self;
}

mirror_enum! {
    /// Mirror of [`RouterMountEntry`]'s variant set.
    RouterMountVariant { Verb, Mount, MountRef, ScopedAttr }
}

impl WireEnum for RouterMountVariant {
    type Wire = RouterMountEntry;
    const SCHEMA_DEF: &'static str = "routerMountEntry";

    fn all() -> &'static [Self] {
        Self::ALL
    }

    fn tag(self) -> &'static str {
        match self {
            Self::Verb => "Verb",
            Self::Mount => "Mount",
            Self::MountRef => "MountRef",
            Self::ScopedAttr => "ScopedAttr",
        }
    }

    fn sample(self) -> RouterMountEntry {
        match self {
            // `attr_keys` is non-empty in every variant that has it, so the
            // `#[serde(default, skip_serializing_if = "Vec::is_empty")]` field actually serializes —
            // the parity probes only ever see fields the fully-populated sample emits.
            Self::Verb => RouterMountEntry::Verb {
                method: "POST".to_string(),
                path: "/setup".to_string(),
                handler: Some("handler".to_string()),
                line: 7,
                attr_keys: vec!["auth-guarded".to_string()],
            },
            Self::Mount => RouterMountEntry::Mount {
                prefix: "/two-factor".to_string(),
                ident: "twoFactorRoute".to_string(),
                specifier: Some("./two-factor".to_string()),
                attr_keys: vec!["auth-guarded".to_string()],
            },
            Self::MountRef => RouterMountEntry::MountRef {
                prefix_ref: "settings.API_V1_STR".to_string(),
                ident: "api_router".to_string(),
                specifier: Some("./api".to_string()),
                attr_keys: vec!["auth-guarded".to_string()],
            },
            Self::ScopedAttr => RouterMountEntry::ScopedAttr {
                prefix: "/admin".to_string(),
                key: "auth-guarded".to_string(),
                line: 3,
            },
        }
    }

    fn of(wire: &RouterMountEntry) -> Self {
        match wire {
            RouterMountEntry::Verb { .. } => Self::Verb,
            RouterMountEntry::Mount { .. } => Self::Mount,
            RouterMountEntry::MountRef { .. } => Self::MountRef,
            RouterMountEntry::ScopedAttr { .. } => Self::ScopedAttr,
        }
    }
}

mirror_enum! {
    /// Mirror of [`ProcedureRouterEntry`]'s variant set.
    ProcedureRouterVariant { Leaf, Ref, Nested }
}

impl WireEnum for ProcedureRouterVariant {
    type Wire = ProcedureRouterEntry;
    const SCHEMA_DEF: &'static str = "procedureRouterEntry";

    fn all() -> &'static [Self] {
        Self::ALL
    }

    fn tag(self) -> &'static str {
        match self {
            Self::Leaf => "Leaf",
            Self::Ref => "Ref",
            Self::Nested => "Nested",
        }
    }

    fn sample(self) -> ProcedureRouterEntry {
        match self {
            Self::Leaf => ProcedureRouterEntry::Leaf {
                key: "get".to_string(),
                verb: "QUERY".to_string(),
                line: 3,
            },
            Self::Ref => ProcedureRouterEntry::Ref {
                key: "sub".to_string(),
                ident: "subRouter".to_string(),
                specifier: Some("./sub".to_string()),
            },
            // Recursive variant: carries a Leaf so `key_parity.rs` can walk the nested shape once.
            Self::Nested => ProcedureRouterEntry::Nested {
                key: "nested".to_string(),
                entries: vec![Self::Leaf.sample()],
            },
        }
    }

    fn of(wire: &ProcedureRouterEntry) -> Self {
        match wire {
            ProcedureRouterEntry::Leaf { .. } => Self::Leaf,
            ProcedureRouterEntry::Ref { .. } => Self::Ref,
            ProcedureRouterEntry::Nested { .. } => Self::Nested,
        }
    }
}

mirror_enum! {
    /// Mirror of [`EntityRef`]'s variant set — the target of an [`Attribute`] on the per-file
    /// `attributes` channel. `EntityRef` is `#[serde(rename_all = "camelCase")]`, so its wire tags are
    /// lowercase-initial; `tag()` spells them out by hand exactly so that rename is under guard.
    EntityRefVariant { File, Symbol, IoKey, PathScope }
}

impl WireEnum for EntityRefVariant {
    type Wire = EntityRef;
    const SCHEMA_DEF: &'static str = "entityRef";

    fn all() -> &'static [Self] {
        Self::ALL
    }

    fn tag(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Symbol => "symbol",
            Self::IoKey => "ioKey",
            Self::PathScope => "pathScope",
        }
    }

    fn sample(self) -> EntityRef {
        match self {
            Self::File => EntityRef::File {
                path: "src/user.controller.ts".to_string(),
            },
            Self::Symbol => EntityRef::Symbol {
                name: "createUser".to_string(),
                file: Some("src/user.service.ts".to_string()),
            },
            Self::IoKey => EntityRef::IoKey {
                kind: "http".to_string(),
                key: "POST /api/users".to_string(),
            },
            Self::PathScope => EntityRef::PathScope {
                prefix: "/admin".to_string(),
            },
        }
    }

    fn of(wire: &EntityRef) -> Self {
        match wire {
            EntityRef::File { .. } => Self::File,
            EntityRef::Symbol { .. } => Self::Symbol,
            EntityRef::IoKey { .. } => Self::IoKey,
            EntityRef::PathScope { .. } => Self::PathScope,
        }
    }
}

/// One [`Attribute`] per [`EntityRef`] variant — the per-file `attributes` channel's sample, so the
/// `attribute`/`entityRef` definitions are exercised by the same walk everything else gets. Derived
/// from `EntityRefVariant::ALL`, so a new `EntityRef` variant lands here with no edit.
pub(crate) fn sample_attributes() -> Vec<Attribute> {
    EntityRefVariant::all()
        .iter()
        .map(|&variant| Attribute {
            target: variant.sample(),
            key: "auth-guarded".to_string(),
            value: serde_json::Value::Bool(true),
        })
        .collect()
}

/// Binds one wire enum's variant set to its schema wrapper definition, in BOTH directions. Returns
/// the number of variants checked so the caller can floor-assert the check is not vacuous.
pub(crate) fn assert_wire_enum_bound<E: WireEnum>(schema: &Value) -> usize {
    let entry_def = def(schema, E::SCHEMA_DEF);
    let schema_variants = props(entry_def);
    let all = E::all();

    // Floor: an empty variant list would make every per-variant assertion below vacuous. (The
    // schema -> Rust diff at the end would also fire, naming every wrapper property — this states the
    // invariant directly rather than relying on that side effect.)
    assert!(
        !all.is_empty(),
        "definitions.{}: the Rust variant list is EMPTY — this check would pass vacuously. \
         A wire enum always has at least one variant.",
        E::SCHEMA_DEF
    );

    let mut rust_tags: HashSet<&str> = HashSet::new();
    for &variant in all {
        let tag = variant.tag();
        let pointer = format!("definitions.{}.properties.{tag}", E::SCHEMA_DEF);
        assert!(
            rust_tags.insert(tag),
            "{pointer}: two Rust variants claim the wire tag '{tag}' — externally-tagged enums \
             cannot share a tag, so one of them can never be deserialized back."
        );

        let produced = serde_json::to_value(variant.sample())
            .unwrap_or_else(|e| panic!("{pointer}: variant sample must serialize: {e}"));
        let produced_obj = obj(&produced, &pointer);
        assert_eq!(
            produced_obj.len(),
            1,
            "{pointer}: an externally-tagged enum instance must serialize as exactly one key, \
             got: {produced}"
        );
        let (serde_tag, inner) = produced_obj.iter().next().expect("len checked above");
        assert_eq!(
            serde_tag.as_str(),
            tag,
            "{pointer}: variant {variant:?} serializes under tag '{serde_tag}', but this file's \
             hand-derived `tag()` says '{tag}' — a #[serde(rename)]/rename_all changed the wire form \
             and the mapping did not follow."
        );

        let variant_schema = schema_variants.get(tag).unwrap_or_else(|| {
            panic!(
                "{pointer}: Rust variant {variant:?} serializes as tag '{tag}', but \
                 docs/adapters/envelope.schema.json's definitions.{}.properties declares only {:?}. \
                 The wrapper is `additionalProperties: false` + `maxProperties: 1`, so an adapter \
                 emitting the shape zzop ITSELF emits would be schema-INVALID. Add the variant to \
                 the schema.",
                E::SCHEMA_DEF,
                schema_variants.keys().collect::<Vec<_>>()
            )
        });
        assert_parity(
            &pointer,
            &format!("{}.{tag}", E::SCHEMA_DEF),
            inner,
            props(variant_schema),
        );

        let round_tripped: E::Wire = serde_json::from_value(produced.clone()).unwrap_or_else(|e| {
            panic!("{pointer}: the sample does not deserialize back into the Rust type: {e}")
        });
        assert_eq!(
            E::of(&round_tripped),
            variant,
            "{pointer}: the sample for {variant:?} round-trips into a DIFFERENT variant — the \
             `sample()` arm builds the wrong variant."
        );
    }

    let schema_tags: HashSet<&str> = schema_variants.keys().map(String::as_str).collect();
    let mut only_in_schema: Vec<&str> = schema_tags.difference(&rust_tags).copied().collect();
    only_in_schema.sort_unstable();
    assert!(
        only_in_schema.is_empty(),
        "definitions.{}: the schema wrapper declares variant(s) {only_in_schema:?} that NO Rust \
         variant produces — a stale variant (renamed or removed upstream), or the Rust-side variant \
         list was emptied.",
        E::SCHEMA_DEF
    );

    all.len()
}

/// Every externally-tagged wire enum, bound variant-for-variant to its schema wrapper — see the
/// module doc for the compile-error chain that makes this impossible to skip.
#[test]
fn wire_enum_variants_are_bound_to_schema_wrappers_bidirectionally() {
    let schema = crate::load_schema();
    let checked = assert_wire_enum_bound::<RouterMountVariant>(&schema)
        + assert_wire_enum_bound::<ProcedureRouterVariant>(&schema)
        + assert_wire_enum_bound::<EntityRefVariant>(&schema);
    assert!(
        checked >= 3,
        "the three bound wire enums together produced only {checked} variant(s) — the variant \
         lists collapsed and this test is checking almost nothing."
    );
}
