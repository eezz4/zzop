//! Fully-populated sample builders — every `Option` `Some`, every `Vec`/`Map` non-empty,
//! recursively, so every `skip_serializing_if`-gated field actually appears in the serialized
//! JSON. See the target doc in `main.rs` for the method.
//!
//! STRUCT shapes are built here. Externally-tagged ENUM shapes are NOT: their per-variant samples
//! live in `wire_variants.rs`, next to the exhaustive match that makes a new variant a compile
//! error, and the containers below derive their entry lists from that one variant list — a
//! hand-written entry list here is exactly how `RouterMountEntry::MountRef` once escaped coverage.

use zzop_core::{
    ClassShapeFragment, ConsumeBodyShape, FileProjection, ImportBinding, IoConsume, IoFacts,
    IoProvide, NonIdempotentKind, NormalizedEnvelope, ProcedureRouterFragment, ProjectionOverrides,
    ProvideBodyField, ProvideBodyShape, ReExport, RouterMountFragment, SourceSymbol,
    SourceSymbolKind, WriteSite, NORMALIZED_AST_FORMAT,
};

use crate::wire_variants::{
    sample_attributes, ProcedureRouterVariant, RouterMountVariant, WireEnum,
};

/// One fully-populated `ProvideBodyField` — every `Option` `Some`, nothing to default.
pub(crate) fn sample_provide_body_field() -> ProvideBodyField {
    ProvideBodyField {
        name: "email".to_string(),
        optional: true,
    }
}

pub(crate) fn sample_provide_body_shape() -> ProvideBodyShape {
    ProvideBodyShape {
        sub_key: Some("user".to_string()),
        dto_ref: Some("CreateUserDto".to_string()),
        fields: vec![sample_provide_body_field()],
        complete: true,
    }
}

pub(crate) fn sample_provide_response_shape() -> zzop_core::ProvideResponseShape {
    zzop_core::ProvideResponseShape {
        dto_ref: Some("UserDto".to_string()),
        fields: vec![sample_provide_body_field()],
        complete: true,
    }
}

pub(crate) fn sample_consume_body_shape() -> ConsumeBodyShape {
    ConsumeBodyShape {
        keys: vec!["user".to_string(), "user.email".to_string()],
        complete_at: vec!["".to_string(), "user".to_string()],
    }
}

pub(crate) fn sample_write_site() -> WriteSite {
    WriteSite {
        file: "src/user.service.ts".to_string(),
        line: 42,
        sink: "prisma.user.update".to_string(),
        kind: Some(NonIdempotentKind::Create),
    }
}

/// One fully-populated `RawCall` — `receiver_type` `Some` and `is_heritage` `true` so both
/// `skip_serializing_if`-gated fields actually appear in the serialized JSON.
pub(crate) fn sample_raw_call() -> zzop_core::callgraph::RawCall {
    zzop_core::callgraph::RawCall {
        from_symbol: "src/user.controller.ts#createUser".to_string(),
        callee_name: "requireAuth".to_string(),
        line: 12,
        receiver_type: Some("AuthService".to_string()),
        is_heritage: true,
    }
}

pub(crate) fn sample_symbol() -> SourceSymbol {
    SourceSymbol {
        id: "src/user.service.ts#createUser".to_string(),
        file: "src/user.service.ts".to_string(),
        name: "createUser".to_string(),
        kind: SourceSymbolKind::Function,
        line: 10,
        exported: true,
        is_default: true,
        body_start: Some(11),
        body_end: Some(20),
        write_sites: vec![sample_write_site()],
    }
}

pub(crate) fn sample_import_binding() -> ImportBinding {
    ImportBinding {
        specifier: "../shared/prisma".to_string(),
        original: "default".to_string(),
        deferred: true,
        type_only: true,
    }
}

pub(crate) fn sample_re_export() -> ReExport {
    ReExport {
        specifier: "./bar".to_string(),
        original: "Bar".to_string(),
        local_alias: "BarAlias".to_string(),
        type_only: true,
    }
}

pub(crate) fn sample_io_provide() -> IoProvide {
    IoProvide {
        kind: "http".to_string(),
        key: "GET /users/{}".to_string(),
        file: "src/user.controller.ts".to_string(),
        line: 15,
        symbol: Some("createUser".to_string()),
        body: Some(sample_provide_body_shape()),
        response: Some(sample_provide_response_shape()),
    }
}

pub(crate) fn sample_io_consume() -> IoConsume {
    IoConsume {
        kind: "http".to_string(),
        key: Some("GET /users/{}".to_string()),
        file: "src/user.client.ts".to_string(),
        line: 25,
        raw: Some("axios.get(url)".to_string()),
        method: Some("GET".to_string()),
        retry_configured: None,
        body: Some(sample_consume_body_shape()),
        client: Some("axios".to_string()),
    }
}

pub(crate) fn sample_class_shape_fragment() -> ClassShapeFragment {
    ClassShapeFragment {
        name: "CreateUserDto".to_string(),
        fields: vec![sample_provide_body_field()],
        complete: true,
    }
}

/// Entries = EVERY `ProcedureRouterEntry` variant, derived from `wire_variants.rs`'s one variant
/// list — so a new variant is walked by the key-set parity guard without an edit here.
pub(crate) fn sample_trpc_fragment() -> ProcedureRouterFragment {
    ProcedureRouterFragment {
        name: "appRouter".to_string(),
        entries: ProcedureRouterVariant::all()
            .iter()
            .map(|&v| v.sample())
            .collect(),
    }
}

/// Entries = EVERY `RouterMountEntry` variant — see [`sample_trpc_fragment`] for why the list is
/// derived rather than written out.
pub(crate) fn sample_router_mount_fragment() -> RouterMountFragment {
    RouterMountFragment {
        name: "auth".to_string(),
        entries: RouterMountVariant::all()
            .iter()
            .map(|&v| v.sample())
            .collect(),
    }
}

/// One fully-populated `FileProjection` — every optional/`#[serde(default)]` field non-default so
/// every one of them actually appears in the serialized JSON.
pub(crate) fn sample_file_projection() -> FileProjection {
    let mut imports = std::collections::BTreeMap::new();
    imports.insert("prisma".to_string(), sample_import_binding());

    let mut const_map_fragment = std::collections::HashMap::new();
    const_map_fragment.insert("USERS_TABLE".to_string(), "users".to_string());

    FileProjection {
        path: "src/user.controller.ts".to_string(),
        loc: 100,
        symbols: vec![sample_symbol()],
        imports,
        re_exports: vec![sample_re_export()],
        dynamic_imports: vec!["./lazy".to_string()],
        used_names: vec!["createUser".to_string()],
        const_map_fragment,
        procedure_router_fragments: vec![sample_trpc_fragment()],
        router_mount_fragments: vec![sample_router_mount_fragment()],
        class_shape_fragments: vec![sample_class_shape_fragment()],
        io: IoFacts {
            provides: vec![sample_io_provide()],
            consumes: vec![sample_io_consume()],
        },
        loop_spans: vec![(10, 20)],
        function_spans: vec![(5, 30)],
        test_spans: vec![(40, 60)],
        calls: vec![sample_raw_call()],
        degraded: true,
        is_entry: true,
        overrides: ProjectionOverrides {
            imports: vec!["displacedLocalName".to_string()],
        },
        // One `Attribute` per `EntityRef` variant. This was `Vec::new()` until the wire-variant
        // binding landed, which meant the `attribute`/`entityRef` definitions — a whole
        // externally-tagged enum on the envelope wire — were documented but never checked against
        // anything.
        attributes: sample_attributes(),
    }
}

pub(crate) fn sample_envelope() -> NormalizedEnvelope {
    NormalizedEnvelope {
        format: NORMALIZED_AST_FORMAT.to_string(),
        version: zzop_core::NORMALIZED_AST_CONTRACT_VERSION.to_string(),
        parser: "test-adapter/1".to_string(),
        source: "test-source".to_string(),
        files: vec![sample_file_projection()],
    }
}
