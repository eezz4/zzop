//! End-to-end test for `controller-prefix-ref-v1`: a NestJS `@Controller(RouteKey.Asset)` whose prefix
//! is a dotted member-expression reference to a STRING `enum` member declared in a DIFFERENT file. Before
//! this feature, `controller_context`'s never-guess rule skipped the whole controller — every route on
//! it silently vanished (the real-corpus gap this closes: immich's `AssetController` alone accounted for
//! 70 unprovided consumes). Pins the composition ORDER against `zzop_engine::analyze_tree`: the
//! `RouteKey.Asset` -> `assets` resolution (`compose::compose_controller_prefix_provides`) must compose
//! BEFORE the NestJS `app.setGlobalPrefix('api')` apply/strip seam, so the final route carries both
//! transforms (`GET /api/assets/{}`), not just one.

use zzop_engine::{analyze_tree, EngineConfig};

use crate::temp_dir::TempDir;

fn config() -> EngineConfig {
    EngineConfig {
        source_id: "fixture".to_string(),
        ..EngineConfig::default()
    }
}

/// `src/enum.ts` declares `RouteKey.Asset`, in a DIFFERENT file from the controller that references it —
/// only assemble-time cross-file composition can resolve this, never the per-file pass.
fn resolvable_fixture() -> TempDir {
    let dir = TempDir::new("zzop-engine-controller-prefix-ref");
    dir.write("src/enum.ts", "export enum RouteKey { Asset = 'assets' }\n");
    dir.write(
        "src/asset.controller.ts",
        concat!(
            "import { Controller, Get, Delete } from '@nestjs/common';\n",
            "import { RouteKey } from './enum';\n\n",
            "@Controller(RouteKey.Asset)\n",
            "export class AssetController {\n",
            "  @Get(':id')\n",
            "  getById() {}\n\n",
            "  @Delete()\n",
            "  remove() {}\n",
            "}\n",
        ),
    );
    dir.write("src/main.ts", "app.setGlobalPrefix('api');\n");
    dir
}

#[test]
fn cross_file_enum_prefix_composes_with_the_global_prefix_in_order() {
    let dir = resolvable_fixture();
    let out = analyze_tree(dir.path(), &config());

    let http_keys: Vec<&str> = out
        .ir
        .ir
        .io
        .as_ref()
        .expect("expected merged IoFacts")
        .provides
        .iter()
        .filter(|p| p.kind == "http")
        .map(|p| p.key.as_str())
        .collect();

    assert!(
        http_keys.contains(&"GET /api/assets/{}"),
        "expected the RouteKey.Asset -> assets -> /api prefix chain to compose, got: {http_keys:?}"
    );
    assert!(
        http_keys.contains(&"DELETE /api/assets"),
        "expected the bare @Delete() route under the resolved+global-prefixed path, got: {http_keys:?}"
    );

    let get_provide = out
        .ir
        .ir
        .io
        .as_ref()
        .unwrap()
        .provides
        .iter()
        .find(|p| p.key == "GET /api/assets/{}")
        .expect("provide present");
    assert_eq!(get_provide.file, "src/asset.controller.ts");
    assert_eq!(get_provide.symbol.as_deref(), Some("getById"));

    assert!(
        !out.warnings.iter().any(|w| w.contains("RouteKey.Asset")),
        "a resolvable prefix ref must not warn: {:?}",
        out.warnings
    );
}

/// No file anywhere declares `RouteKey` — the prefix ref can never resolve, so this controller's routes
/// must never be guess-emitted (never-guess), and a warning must name the ref, the file, and the route
/// count.
#[test]
fn unresolvable_enum_prefix_yields_zero_provides_and_a_warning() {
    let dir = TempDir::new("zzop-engine-controller-prefix-ref-unresolved");
    dir.write(
        "src/asset.controller.ts",
        concat!(
            "import { Controller, Get, Delete } from '@nestjs/common';\n\n",
            "@Controller(RouteKey.Asset)\n",
            "export class AssetController {\n",
            "  @Get(':id')\n",
            "  getById() {}\n\n",
            "  @Delete()\n",
            "  remove() {}\n",
            "}\n",
        ),
    );

    let out = analyze_tree(dir.path(), &config());

    let http_provides: Vec<_> = out
        .ir
        .ir
        .io
        .as_ref()
        .map(|io| {
            io.provides
                .iter()
                .filter(|p| p.kind == "http")
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    assert!(
        http_provides.is_empty(),
        "an unresolvable controller prefix must never guess-emit a route, got: {http_provides:?}"
    );

    let warning = out
        .warnings
        .iter()
        .find(|w| w.contains("RouteKey.Asset"))
        .unwrap_or_else(|| {
            panic!(
                "expected an unresolved-prefix warning, got: {:?}",
                out.warnings
            )
        });
    assert!(warning.contains("src/asset.controller.ts"), "{warning}");
    assert!(warning.contains("2 routes"), "{warning}");
}
