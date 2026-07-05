//! End-to-end coverage for the BE-framework coverage self-report (`zpz_engine::coverage`,
//! `controller_silence_warning`): a tree whose files carry controller-decorator-shaped lines in 3+ distinct
//! files, yet extract ZERO `http` provides tree-wide, gets an `AnalyzeOutput::warnings` entry naming the
//! gap. Guards against an unrecognized controller-decorator convention (e.g. a framework using
//! `@RestController` under a different package name, the way n8n's own `@n8n/decorators` does) failing
//! silently instead of surfacing a coverage warning — the NEXT unknown framework at least gets a warning
//! instead of silent cross-layer-join darkness.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zpz_engine::{analyze_tree, EngineConfig};

struct TempDir(PathBuf);

impl TempDir {
    fn new(prefix: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("{prefix}-{}-{nanos}-{n}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn write(&self, rel: &str, content: &str) {
        let full = self.0.join(rel);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(full, content).unwrap();
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn config() -> EngineConfig {
    EngineConfig {
        source_id: "coverage-warning-fixture".to_string(),
        ..EngineConfig::default()
    }
}

const WARNING_SUBSTRING: &str = "route decorators/annotations but no http routes were extracted";

/// 3 files carrying an invented `@FastController`/`@FastGet` decorator shape — structurally identical
/// (class-level gate + method-level verb) to Nest's own idiom, but under decorator NAMES that
/// `zpz_parser_typescript::nest`'s `CONTROLLER_CLASS_GATES` (`["Controller", "RestController"]`, exact-name
/// matched) does not recognize, and that don't match the Spring extractor's regex either. No Nest/Spring/
/// Hono shape appears anywhere in this tree, so this tree's real http-provides count is genuinely zero.
fn unrecognized_framework_tree() -> TempDir {
    let dir = TempDir::new("zpz-engine-coverage-unrecognized");
    dir.write(
        "src/users.controller.ts",
        "@FastController('/users')\nexport class UsersController {\n  @FastGet('/')\n  findAll() { return []; }\n}\n",
    );
    dir.write(
        "src/orders.controller.ts",
        "@FastController('/orders')\nexport class OrdersController {\n  @FastGet('/')\n  findAll() { return []; }\n}\n",
    );
    dir.write(
        "src/items.controller.ts",
        "@FastController('/items')\nexport class ItemsController {\n  @FastGet('/')\n  findAll() { return []; }\n}\n",
    );
    dir
}

/// A real NestJS-shaped BE tree (`@Controller`/`@Get`) — `zpz_parser_typescript::nest::extract_nest_provides`
/// recognizes this shape and extracts a real `http` provide, so the tree's http-provides count is > 0 and
/// `controller_silence_warning` short-circuits before ever reading a file. Mirrors
/// `analyze_multi_tree_nestjs.rs`'s `nest_be_tree()` helper (adapted to a single-tree `analyze_tree` call
/// rather than the cross-layer `analyze_trees` API that test exercises).
fn real_nest_tree() -> TempDir {
    let dir = TempDir::new("zpz-engine-coverage-real-nest");
    dir.write(
        "src/users/users.controller.ts",
        concat!(
            "import { Controller, Get, Param } from '@nestjs/common';\n\n",
            "@Controller('api/users')\n",
            "export class UsersController {\n",
            "  @Get(':id')\n",
            "  findOne(@Param('id') id: string) {\n    return id;\n  }\n",
            "}\n",
        ),
    );
    dir
}

/// A pure-FE Angular-style fixture (`@Component`/`@Input`/`@Output`/`@HostListener`) — none of Angular's own
/// decorator vocabulary lexically matches `Controller`/`Mapping`/`Get`/`Post`/`Put`/`Delete`/`Patch`, so the
/// regex never matches at all (verified in `zpz_engine::coverage`'s own unit tests too); this proves the
/// engine-level wiring inherits that same no-false-positive property, not just the bare function in
/// isolation.
fn angular_fe_tree() -> TempDir {
    let dir = TempDir::new("zpz-engine-coverage-angular");
    dir.write(
        "src/a.component.ts",
        "@Component({ selector: 'app-a' })\nexport class AComponent {\n  @Input() x: string;\n  @Output() y = new EventEmitter();\n  @HostListener('click')\n  onClick() {}\n}\n",
    );
    dir.write(
        "src/b.component.ts",
        "@Component({ selector: 'app-b' })\nexport class BComponent {\n  @Inject(TOKEN) dep: any;\n}\n",
    );
    dir.write(
        "src/c.component.ts",
        "@Component({ selector: 'app-c' })\nexport class CComponent {\n  @Input() z: number;\n}\n",
    );
    dir
}

#[test]
fn unrecognized_controller_decorator_shape_with_zero_http_provides_warns() {
    let dir = unrecognized_framework_tree();
    let out = analyze_tree(dir.path(), &config());

    assert!(
        out.warnings.iter().any(|w| w.contains(WARNING_SUBSTRING)),
        "expected the coverage-gap warning, got: {:?}",
        out.warnings
    );
}

#[test]
fn a_real_nest_tree_extracts_provides_and_never_warns() {
    let dir = real_nest_tree();
    let out = analyze_tree(dir.path(), &config());

    assert!(
        out.ir
            .ir
            .io
            .as_ref()
            .is_some_and(|io| io.provides.iter().any(|p| p.kind == "http")),
        "expected at least one real http provide from the Nest fixture, got: {:?}",
        out.ir.ir.io
    );
    assert!(
        !out.warnings.iter().any(|w| w.contains(WARNING_SUBSTRING)),
        "a tree that successfully extracts http provides must never get the coverage-gap warning, got: {:?}",
        out.warnings
    );
}

#[test]
fn a_pure_fe_angular_tree_never_warns() {
    let dir = angular_fe_tree();
    let out = analyze_tree(dir.path(), &config());

    assert!(
        !out.warnings.iter().any(|w| w.contains(WARNING_SUBSTRING)),
        "Angular's decorator vocabulary must never trigger the coverage-gap warning, got: {:?}",
        out.warnings
    );
}

#[test]
fn two_runs_over_the_unrecognized_framework_tree_produce_identical_warnings() {
    let dir = unrecognized_framework_tree();
    let cfg = config();
    let out1 = analyze_tree(dir.path(), &cfg);
    let out2 = analyze_tree(dir.path(), &cfg);
    assert_eq!(out1.warnings, out2.warnings);
}
