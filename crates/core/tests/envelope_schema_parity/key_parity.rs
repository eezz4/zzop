//! The main key-set parity guard — the bidirectional key-NAME diff walked position-by-position.
//! See the target doc in `main.rs` for the method.

use crate::samples::sample_envelope;
use crate::{
    assert_all_variants_covered, assert_parity, assert_variant_tag_known, def, field, idx,
    load_schema, obj, props,
};

/// The main parity guard: serializes one fully-populated `NormalizedEnvelope`, then walks it
/// exactly (JSON pointer commented at each step) against the schema definition that describes that
/// exact position.
#[test]
fn envelope_schema_matches_normalized_envelope_field_for_field() {
    let schema = load_schema();
    let sample = serde_json::to_value(sample_envelope()).expect("sample envelope must serialize");

    // $ (root) -> top-level `properties` (NormalizedEnvelope has no optional fields at all).
    let root_props = props(&schema);
    assert_parity("$", "<root>", &sample, root_props);

    // $.files[0] -> definitions.fileProjection
    let file0 = idx(field(&sample, "files", "$"), 0, "$.files");
    assert_parity(
        "$.files[0]",
        "fileProjection",
        file0,
        props(def(&schema, "fileProjection")),
    );

    // $.files[0].symbols[0] -> definitions.sourceSymbol
    let symbol0 = idx(
        field(file0, "symbols", "$.files[0]"),
        0,
        "$.files[0].symbols",
    );
    assert_parity(
        "$.files[0].symbols[0]",
        "sourceSymbol",
        symbol0,
        props(def(&schema, "sourceSymbol")),
    );

    // $.files[0].symbols[0].writeSites[0] -> definitions.writeSite (wire name is camelCase
    // writeSites — SourceSymbol is #[serde(rename_all = "camelCase")]).
    let write_site0 = idx(
        field(symbol0, "writeSites", "$.files[0].symbols[0]"),
        0,
        "$.files[0].symbols[0].writeSites",
    );
    assert_parity(
        "$.files[0].symbols[0].writeSites[0]",
        "writeSite",
        write_site0,
        props(def(&schema, "writeSite")),
    );

    // $.files[0].imports.prisma -> definitions.importBinding (ImportMap is keyed by localName;
    // the schema models this via `additionalProperties: { $ref: importBinding }`, not a `properties`
    // entry — take the one value we inserted).
    let imports_obj = obj(field(file0, "imports", "$.files[0]"), "$.files[0].imports");
    let import_binding0 = imports_obj
        .get("prisma")
        .expect("sample must carry the 'prisma' import binding");
    assert_parity(
        "$.files[0].imports.prisma",
        "importBinding",
        import_binding0,
        props(def(&schema, "importBinding")),
    );

    // $.files[0].re_exports[0] -> definitions.reExport
    let re_export0 = idx(
        field(file0, "re_exports", "$.files[0]"),
        0,
        "$.files[0].re_exports",
    );
    assert_parity(
        "$.files[0].re_exports[0]",
        "reExport",
        re_export0,
        props(def(&schema, "reExport")),
    );

    // $.files[0].class_shape_fragments[0] -> definitions.classShapeFragment
    let class_shape0 = idx(
        field(file0, "class_shape_fragments", "$.files[0]"),
        0,
        "$.files[0].class_shape_fragments",
    );
    assert_parity(
        "$.files[0].class_shape_fragments[0]",
        "classShapeFragment",
        class_shape0,
        props(def(&schema, "classShapeFragment")),
    );
    // $.files[0].class_shape_fragments[0].fields[0] -> definitions.provideBodyField (shared type)
    let class_shape_field0 = idx(
        field(
            class_shape0,
            "fields",
            "$.files[0].class_shape_fragments[0]",
        ),
        0,
        "$.files[0].class_shape_fragments[0].fields",
    );
    assert_parity(
        "$.files[0].class_shape_fragments[0].fields[0]",
        "provideBodyField",
        class_shape_field0,
        props(def(&schema, "provideBodyField")),
    );

    // $.files[0].io -> definitions.ioFacts
    let io0 = field(file0, "io", "$.files[0]");
    assert_parity(
        "$.files[0].io",
        "ioFacts",
        io0,
        props(def(&schema, "ioFacts")),
    );

    // $.files[0].io.provides[0] -> definitions.ioProvide
    let provide0 = idx(
        field(io0, "provides", "$.files[0].io"),
        0,
        "$.files[0].io.provides",
    );
    assert_parity(
        "$.files[0].io.provides[0]",
        "ioProvide",
        provide0,
        props(def(&schema, "ioProvide")),
    );
    // $.files[0].io.provides[0].body -> definitions.provideBodyShape
    let provide_body0 = field(provide0, "body", "$.files[0].io.provides[0]");
    assert_parity(
        "$.files[0].io.provides[0].body",
        "provideBodyShape",
        provide_body0,
        props(def(&schema, "provideBodyShape")),
    );
    // $.files[0].io.provides[0].body.fields[0] -> definitions.provideBodyField
    let provide_body_field0 = idx(
        field(provide_body0, "fields", "$.files[0].io.provides[0].body"),
        0,
        "$.files[0].io.provides[0].body.fields",
    );
    assert_parity(
        "$.files[0].io.provides[0].body.fields[0]",
        "provideBodyField",
        provide_body_field0,
        props(def(&schema, "provideBodyField")),
    );

    // $.files[0].io.consumes[0] -> definitions.ioConsume
    let consume0 = idx(
        field(io0, "consumes", "$.files[0].io"),
        0,
        "$.files[0].io.consumes",
    );
    assert_parity(
        "$.files[0].io.consumes[0]",
        "ioConsume",
        consume0,
        props(def(&schema, "ioConsume")),
    );
    // $.files[0].io.consumes[0].body -> definitions.consumeBodyShape
    let consume_body0 = field(consume0, "body", "$.files[0].io.consumes[0]");
    assert_parity(
        "$.files[0].io.consumes[0].body",
        "consumeBodyShape",
        consume_body0,
        props(def(&schema, "consumeBodyShape")),
    );

    // $.files[0].procedure_router_fragments[0] -> definitions.procedureRouterFragment
    let trpc_fragment0 = idx(
        field(file0, "procedure_router_fragments", "$.files[0]"),
        0,
        "$.files[0].procedure_router_fragments",
    );
    assert_parity(
        "$.files[0].procedure_router_fragments[0]",
        "procedureRouterFragment",
        trpc_fragment0,
        props(def(&schema, "procedureRouterFragment")),
    );
    // $.files[0].procedure_router_fragments[0].entries[*] -> definitions.procedureRouterEntry (externally
    // tagged enum: Leaf/Ref/Nested). The fixture carries all three variants; Nested additionally
    // recurses into its own `entries[0]` (a Leaf) to exercise the recursive shape once.
    let trpc_entries = field(
        trpc_fragment0,
        "entries",
        "$.files[0].procedure_router_fragments[0]",
    )
    .as_array()
    .expect("entries must be an array");
    let trpc_entry_def = def(&schema, "procedureRouterEntry");
    let mut trpc_tags_seen = Vec::new();
    for (i, entry) in trpc_entries.iter().enumerate() {
        let pointer = format!("$.files[0].procedure_router_fragments[0].entries[{i}]");
        let (tag, inner) = assert_variant_tag_known(&pointer, entry, trpc_entry_def);
        trpc_tags_seen.push(tag);
        let variant_props = props(
            field(trpc_entry_def, "properties", &pointer)
                .get(tag)
                .unwrap(),
        );
        assert_parity(
            &format!("{pointer}.{tag}"),
            &format!("procedureRouterEntry.{tag}"),
            inner,
            variant_props,
        );
        if tag == "Nested" {
            let nested_entry0 = idx(
                field(inner, "entries", &pointer),
                0,
                &format!("{pointer}.entries"),
            );
            let nested_pointer = format!("{pointer}.entries[0]");
            let (nested_tag, nested_inner) =
                assert_variant_tag_known(&nested_pointer, nested_entry0, trpc_entry_def);
            trpc_tags_seen.push(nested_tag);
            let nested_variant_props = props(
                field(trpc_entry_def, "properties", &nested_pointer)
                    .get(nested_tag)
                    .unwrap(),
            );
            assert_parity(
                &format!("{nested_pointer}.{nested_tag}"),
                &format!("procedureRouterEntry.{nested_tag}"),
                nested_inner,
                nested_variant_props,
            );
        }
    }
    assert_all_variants_covered(
        "$.files[0].procedure_router_fragments[0].entries[*]",
        trpc_entry_def,
        &trpc_tags_seen,
    );

    // $.files[0].router_mount_fragments[0] -> definitions.routerMountFragment
    let mount_fragment0 = idx(
        field(file0, "router_mount_fragments", "$.files[0]"),
        0,
        "$.files[0].router_mount_fragments",
    );
    assert_parity(
        "$.files[0].router_mount_fragments[0]",
        "routerMountFragment",
        mount_fragment0,
        props(def(&schema, "routerMountFragment")),
    );
    // $.files[0].router_mount_fragments[0].entries[*] -> definitions.routerMountEntry (Verb/Mount)
    let mount_entries = field(
        mount_fragment0,
        "entries",
        "$.files[0].router_mount_fragments[0]",
    )
    .as_array()
    .expect("entries must be an array");
    let mount_entry_def = def(&schema, "routerMountEntry");
    let mut mount_tags_seen = Vec::new();
    for (i, entry) in mount_entries.iter().enumerate() {
        let pointer = format!("$.files[0].router_mount_fragments[0].entries[{i}]");
        let (tag, inner) = assert_variant_tag_known(&pointer, entry, mount_entry_def);
        mount_tags_seen.push(tag);
        let variant_props = props(
            field(mount_entry_def, "properties", &pointer)
                .get(tag)
                .unwrap(),
        );
        assert_parity(
            &format!("{pointer}.{tag}"),
            &format!("routerMountEntry.{tag}"),
            inner,
            variant_props,
        );
    }
    assert_all_variants_covered(
        "$.files[0].router_mount_fragments[0].entries[*]",
        mount_entry_def,
        &mount_tags_seen,
    );
}
