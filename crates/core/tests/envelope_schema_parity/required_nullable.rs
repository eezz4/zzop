//! The required-ness + nullability parity guard — see the target doc in `main.rs`, "Beyond
//! key-NAME presence" section, for method.

use serde_json::Value;

use zzop_core::IoFacts;

use crate::probes::{
    assert_required_and_nullable_parity, assert_variant_required_and_nullable_parity,
};
use crate::samples::{
    sample_class_shape_fragment, sample_consume_body_shape, sample_envelope,
    sample_file_projection, sample_import_binding, sample_io_consume, sample_io_provide,
    sample_provide_body_field, sample_provide_body_shape, sample_provide_response_shape,
    sample_raw_call, sample_re_export, sample_router_mount_fragment, sample_symbol,
    sample_trpc_fragment, sample_write_site,
};
use crate::wire_variants::{
    EntityRefVariant, ProcedureRouterVariant, RouterMountVariant, WireEnum,
};
use crate::{def, field, load_schema};

/// Probes EVERY variant of one externally-tagged wire enum — each variant carries its own
/// independent `required`/`properties`, so each needs its own probe. The variant list comes from
/// `wire_variants.rs`, the single compiler-enforced one: a new variant is probed here with no edit,
/// where the hand-written per-variant call list this replaced had already silently skipped
/// `routerMountEntry.ScopedAttr`.
fn assert_variants_required_and_nullable<E: WireEnum>(schema: &Value, pointer_base: &str) {
    let entry_def = def(schema, E::SCHEMA_DEF);
    for &variant in E::all() {
        let tag = variant.tag();
        let variant_def = field(
            entry_def,
            "properties",
            &format!("$.definitions.{}", E::SCHEMA_DEF),
        )
        .get(tag)
        .unwrap_or_else(|| panic!("definitions.{}.properties.{tag} must exist", E::SCHEMA_DEF));
        assert_variant_required_and_nullable_parity(
            &format!("{pointer_base}[{tag}]"),
            &format!("{}.{tag}", E::SCHEMA_DEF),
            tag,
            &variant.sample(),
            variant_def,
        );
    }
}

/// The required-ness + nullability parity guard — see the module doc's "Beyond key-NAME presence"
/// section for method. One call per schema definition, plus the schema root, plus one probe per
/// externally-tagged enum VARIANT via [`assert_variants_required_and_nullable`].
#[test]
fn envelope_schema_required_and_nullability_matches_rust_types() {
    let schema = load_schema();

    assert_required_and_nullable_parity("$", "<root>", &sample_envelope(), &schema);

    assert_required_and_nullable_parity(
        "$.files[0]",
        "fileProjection",
        &sample_file_projection(),
        def(&schema, "fileProjection"),
    );

    assert_required_and_nullable_parity(
        "$.files[0].symbols[0]",
        "sourceSymbol",
        &sample_symbol(),
        def(&schema, "sourceSymbol"),
    );

    assert_required_and_nullable_parity(
        "$.files[0].symbols[0].writeSites[0]",
        "writeSite",
        &sample_write_site(),
        def(&schema, "writeSite"),
    );

    assert_required_and_nullable_parity(
        "$.files[0].calls[0]",
        "rawCall",
        &sample_raw_call(),
        def(&schema, "rawCall"),
    );

    assert_required_and_nullable_parity(
        "$.files[0].imports.prisma",
        "importBinding",
        &sample_import_binding(),
        def(&schema, "importBinding"),
    );

    assert_required_and_nullable_parity(
        "$.files[0].re_exports[0]",
        "reExport",
        &sample_re_export(),
        def(&schema, "reExport"),
    );

    assert_required_and_nullable_parity(
        "$.files[0].class_shape_fragments[0]",
        "classShapeFragment",
        &sample_class_shape_fragment(),
        def(&schema, "classShapeFragment"),
    );

    // IoFacts has no dedicated `sample_*` builder (it's assembled inline in `sample_file_projection`);
    // build the same shape here so the probe has a real, fully-populated `IoFacts` value.
    let io_facts_sample = IoFacts {
        provides: vec![sample_io_provide()],
        consumes: vec![sample_io_consume()],
    };
    assert_required_and_nullable_parity(
        "$.files[0].io",
        "ioFacts",
        &io_facts_sample,
        def(&schema, "ioFacts"),
    );

    assert_required_and_nullable_parity(
        "$.files[0].io.provides[0]",
        "ioProvide",
        &sample_io_provide(),
        def(&schema, "ioProvide"),
    );

    assert_required_and_nullable_parity(
        "$.files[0].io.provides[0].body",
        "provideBodyShape",
        &sample_provide_body_shape(),
        def(&schema, "provideBodyShape"),
    );

    assert_required_and_nullable_parity(
        "$.files[0].io.provides[0].body.fields[0]",
        "provideBodyField",
        &sample_provide_body_field(),
        def(&schema, "provideBodyField"),
    );

    assert_required_and_nullable_parity(
        "$.files[0].io.provides[0].response",
        "provideResponseShape",
        &sample_provide_response_shape(),
        def(&schema, "provideResponseShape"),
    );

    assert_required_and_nullable_parity(
        "$.files[0].io.consumes[0]",
        "ioConsume",
        &sample_io_consume(),
        def(&schema, "ioConsume"),
    );

    assert_required_and_nullable_parity(
        "$.files[0].io.consumes[0].body",
        "consumeBodyShape",
        &sample_consume_body_shape(),
        def(&schema, "consumeBodyShape"),
    );

    assert_required_and_nullable_parity(
        "$.files[0].procedure_router_fragments[0]",
        "procedureRouterFragment",
        &sample_trpc_fragment(),
        def(&schema, "procedureRouterFragment"),
    );

    assert_variants_required_and_nullable::<ProcedureRouterVariant>(
        &schema,
        "$.files[0].procedure_router_fragments[0].entries",
    );

    assert_required_and_nullable_parity(
        "$.files[0].router_mount_fragments[0]",
        "routerMountFragment",
        &sample_router_mount_fragment(),
        def(&schema, "routerMountFragment"),
    );

    assert_variants_required_and_nullable::<RouterMountVariant>(
        &schema,
        "$.files[0].router_mount_fragments[0].entries",
    );

    // `Attribute` is a plain struct; only its `target` is an enum, probed per-variant just below.
    // `.first()` rather than `[0]`, so a collapsed sample list reports WHY instead of an index panic.
    let attribute_sample = crate::wire_variants::sample_attributes();
    assert_required_and_nullable_parity(
        "$.files[0].attributes[0]",
        "attribute",
        attribute_sample
            .first()
            .expect("the attributes fixture must carry at least one Attribute"),
        def(&schema, "attribute"),
    );

    assert_variants_required_and_nullable::<EntityRefVariant>(
        &schema,
        "$.files[0].attributes[*].target",
    );
}
