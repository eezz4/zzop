//! The required-ness + nullability parity guard — see the target doc in `main.rs`, "Beyond
//! key-NAME presence" section, for method.

use zzop_core::IoFacts;

use crate::probes::{
    assert_required_and_nullable_parity, assert_variant_required_and_nullable_parity,
};
use crate::samples::{
    sample_class_shape_fragment, sample_consume_body_shape, sample_envelope,
    sample_file_projection, sample_import_binding, sample_io_consume, sample_io_provide,
    sample_mount_mount, sample_mount_verb, sample_provide_body_field, sample_provide_body_shape,
    sample_re_export, sample_router_mount_fragment, sample_symbol, sample_trpc_fragment,
    sample_trpc_leaf, sample_trpc_nested, sample_trpc_ref, sample_write_site,
};
use crate::{def, field, load_schema};

/// The required-ness + nullability parity guard — see the module doc's "Beyond key-NAME presence"
/// section for method. One call per schema definition (16 total), plus the schema root, plus one call
/// per externally-tagged enum VARIANT (`procedureRouterEntry`'s Leaf/Ref/Nested, `routerMountEntry`'s
/// Verb/Mount — each variant has its own independent `required`/`properties`, so each needs its own
/// probe).
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

    let trpc_entry_def = def(&schema, "procedureRouterEntry");
    let trpc_variant_def = |tag: &str| {
        field(
            trpc_entry_def,
            "properties",
            "$.definitions.procedureRouterEntry",
        )
        .get(tag)
        .unwrap_or_else(|| panic!("definitions.procedureRouterEntry.properties.{tag} must exist"))
    };
    assert_variant_required_and_nullable_parity(
        "$.files[0].procedure_router_fragments[0].entries[Leaf]",
        "procedureRouterEntry.Leaf",
        "Leaf",
        &sample_trpc_leaf(),
        trpc_variant_def("Leaf"),
    );
    assert_variant_required_and_nullable_parity(
        "$.files[0].procedure_router_fragments[0].entries[Ref]",
        "procedureRouterEntry.Ref",
        "Ref",
        &sample_trpc_ref(),
        trpc_variant_def("Ref"),
    );
    assert_variant_required_and_nullable_parity(
        "$.files[0].procedure_router_fragments[0].entries[Nested]",
        "procedureRouterEntry.Nested",
        "Nested",
        &sample_trpc_nested(),
        trpc_variant_def("Nested"),
    );

    assert_required_and_nullable_parity(
        "$.files[0].router_mount_fragments[0]",
        "routerMountFragment",
        &sample_router_mount_fragment(),
        def(&schema, "routerMountFragment"),
    );

    let mount_entry_def = def(&schema, "routerMountEntry");
    let mount_variant_def = |tag: &str| {
        field(
            mount_entry_def,
            "properties",
            "$.definitions.routerMountEntry",
        )
        .get(tag)
        .unwrap_or_else(|| panic!("definitions.routerMountEntry.properties.{tag} must exist"))
    };
    assert_variant_required_and_nullable_parity(
        "$.files[0].router_mount_fragments[0].entries[Verb]",
        "routerMountEntry.Verb",
        "Verb",
        &sample_mount_verb(),
        mount_variant_def("Verb"),
    );
    assert_variant_required_and_nullable_parity(
        "$.files[0].router_mount_fragments[0].entries[Mount]",
        "routerMountEntry.Mount",
        "Mount",
        &sample_mount_mount(),
        mount_variant_def("Mount"),
    );
}
