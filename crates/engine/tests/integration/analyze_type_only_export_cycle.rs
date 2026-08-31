//! End-to-end pins for the EXPORT-side half of the type-only cycle exclusion: an import spelled as a
//! plain value import (`import { X } from './y'`) whose EXPORT DECLARATION in `y` is `export type X` /
//! `export interface X` is erased by TypeScript at compile time exactly as `import type { X }` is, so
//! the pair carries no runtime module-load edge and must not read as a circular dependency.
//!
//! The importing-side half (`import type` / per-specifier `{ type X }`) was already handled; these
//! tests cover the case the engine could not see, and — more of them — the four guards that keep the
//! widened exclusion from DELETING a real cycle. Every guard test asserts the cycle SURVIVES: an
//! exclusion that fires on absent or wrong evidence removes a finding and leaves no trace, so the bar
//! for DELETING is a declaration in the source, never an inference about how a name is used.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_engine::{analyze_tree, EngineConfig};

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
        source_id: "fixture".to_string(),
        ..EngineConfig::default()
    }
}

fn circular_files(out: &zzop_engine::AnalyzeOutput) -> Vec<String> {
    out.findings
        .iter()
        .filter(|f| f.rule_id == "circular")
        .map(|f| f.file.clone())
        .collect()
}

/// The corpus shape this whole batch exists for (cal.com `calendars.controller.ts` <->
/// `outlook.service.ts`): the controller imports the service as a value (real runtime edge), and the
/// service imports `CalendarState` back with NO `type` keyword — but the controller DECLARES it
/// `export interface`, so `tsc` erases that import and no module-load cycle exists at runtime.
#[test]
fn a_value_spelled_import_of_an_exported_interface_is_not_a_cycle() {
    let dir = TempDir::new("zzop-engine-typeexport-cycle");
    dir.write(
        "src/calendars.controller.ts",
        "import { OutlookService } from './outlook.service';\n\
         export interface CalendarState { code: string }\n\
         export class CalendarsController {\n\
         \x20 constructor(private readonly svc: OutlookService) {}\n\
         }\n",
    );
    dir.write(
        "src/outlook.service.ts",
        "import { CalendarState } from './calendars.controller';\n\
         export class OutlookService {\n\
         \x20 build(): CalendarState { return { code: 'x' }; }\n\
         }\n",
    );
    let out = analyze_tree(dir.path(), &config());
    assert!(
        circular_files(&out).is_empty(),
        "the back edge is an erased type import (`export interface CalendarState`), so no runtime \
         cycle exists — got: {:?}",
        circular_files(&out)
    );
}

/// The control the measurement plan named (cal.com `bookings.service.ts` <-> `input.service.ts`): the
/// SAME target is imported for a type AND for a runtime `const` in one statement. "Every binding to
/// this target is erasable" is exactly what fails here, and the cycle must survive.
#[test]
fn a_target_imported_for_both_a_type_and_a_value_keeps_the_cycle() {
    let dir = TempDir::new("zzop-engine-typeexport-mixed");
    dir.write(
        "src/bookings.service.ts",
        "import { EventTypeWithOwner, eventTypeBookingFieldsSchema } from './input.service';\n\
         export class BookingsService {\n\
         \x20 fields = eventTypeBookingFieldsSchema;\n\
         \x20 owner: EventTypeWithOwner | null = null;\n\
         }\n",
    );
    dir.write(
        "src/input.service.ts",
        "import { BookingsService } from './bookings.service';\n\
         export type EventTypeWithOwner = { id: number };\n\
         export const eventTypeBookingFieldsSchema = [1, 2, 3];\n\
         export class InputService {\n\
         \x20 constructor(private readonly b: BookingsService) {}\n\
         }\n",
    );
    let out = analyze_tree(dir.path(), &config());
    assert!(
        !circular_files(&out).is_empty(),
        "the mixed value+type import is a REAL runtime edge — this cycle must survive the widened \
         exclusion, got no circular findings"
    );
}

/// Guard (b): positive evidence only. A TypeScript `enum` is a RUNTIME value under
/// `emitDecoratorMetadata`, and the TypeScript front end emits NO symbol for it at all
/// (`parser/parser-typescript/src/symbols.rs`'s `Decl::TsEnum` falls into the `_ => {}` arm). Reading
/// that ABSENCE as "it must be a type" would delete real cycles; absence must always mean "keep the
/// edge". A degraded (lexical-fallback) file, whose `symbols` are likewise empty, is protected by this
/// same arm.
#[test]
fn an_exported_enum_projects_no_symbol_and_therefore_keeps_the_cycle() {
    let dir = TempDir::new("zzop-engine-typeexport-enum");
    dir.write(
        "src/locale.controller.ts",
        "import { Locales } from './locale.enum';\n\
         export class LocaleController {\n\
         \x20 fallback = Locales.En;\n\
         }\n",
    );
    dir.write(
        "src/locale.enum.ts",
        "import { LocaleController } from './locale.controller';\n\
         export enum Locales { En = 'en' }\n\
         export const owner: LocaleController | null = null;\n",
    );
    let out = analyze_tree(dir.path(), &config());
    assert!(
        !circular_files(&out).is_empty(),
        "an `enum` survives to runtime and projects no symbol — its absence must NOT be read as \
         evidence of a type, got no circular findings"
    );
}

/// Guard (d): TypeScript declaration merging. `interface Merged` and `const Merged` legally share a
/// name in one file, and `SourceSymbol::id` deliberately collapses them onto one id
/// (`crates/core/src/ir.rs`), so the export lookup must key on `(file, name)` and require EVERY symbol
/// under it to be a type. One value declaration means the import survives to runtime.
#[test]
fn a_declaration_merged_interface_and_const_keeps_the_cycle() {
    let dir = TempDir::new("zzop-engine-typeexport-merged");
    dir.write(
        "src/merged.controller.ts",
        "import { Merged } from './merged.model';\n\
         export class MergedController {\n\
         \x20 seed = Merged.a;\n\
         }\n",
    );
    dir.write(
        "src/merged.model.ts",
        "import { MergedController } from './merged.controller';\n\
         export interface Merged { a: number }\n\
         export const Merged = { a: 1 };\n\
         export const owner: MergedController | null = null;\n",
    );
    let out = analyze_tree(dir.path(), &config());
    assert!(
        !circular_files(&out).is_empty(),
        "`const Merged` merges with `interface Merged` and survives to runtime — the pair must not \
         be excluded, got no circular findings"
    );
}

/// Guard (c): `verbatimModuleSyntax` (and its predecessors `preserveValueImports` /
/// `importsNotUsedAsValues: "preserve"`) makes `tsc` EMIT `import { X }` verbatim even when X is a
/// type — the module load happens, and the cycle is real. A tree whose governing tsconfig sets any of
/// the three turns this export-side gate off wholesale.
#[test]
fn verbatim_module_syntax_turns_the_export_side_gate_off() {
    let dir = TempDir::new("zzop-engine-typeexport-verbatim");
    dir.write(
        "tsconfig.json",
        r#"{"compilerOptions": {"verbatimModuleSyntax": true}}"#,
    );
    dir.write(
        "src/calendars.controller.ts",
        "import { OutlookService } from './outlook.service';\n\
         export interface CalendarState { code: string }\n\
         export class CalendarsController {\n\
         \x20 constructor(private readonly svc: OutlookService) {}\n\
         }\n",
    );
    dir.write(
        "src/outlook.service.ts",
        "import { CalendarState } from './calendars.controller';\n\
         export class OutlookService {\n\
         \x20 build(): CalendarState { return { code: 'x' }; }\n\
         }\n",
    );
    let out = analyze_tree(dir.path(), &config());
    assert!(
        !circular_files(&out).is_empty(),
        "under verbatimModuleSyntax the import statement is emitted verbatim, so the module-load \
         cycle is real and must still be reported, got no circular findings"
    );
}

/// The importing-side half stays intact and keeps working through the shared fold: `import type`
/// still excludes the edge with no export-side evidence needed at all.
#[test]
fn the_import_side_type_keyword_still_excludes_on_its_own() {
    let dir = TempDir::new("zzop-engine-typeexport-importside");
    dir.write(
        "src/gcal.controller.ts",
        "import { GcalService } from './gcal.service';\n\
         export interface GcalState { code: string }\n\
         export class GcalController {\n\
         \x20 constructor(private readonly svc: GcalService) {}\n\
         }\n",
    );
    dir.write(
        "src/gcal.service.ts",
        "import type { GcalState } from './gcal.controller';\n\
         export class GcalService {\n\
         \x20 build(): GcalState { return { code: 'x' }; }\n\
         }\n",
    );
    let out = analyze_tree(dir.path(), &config());
    assert!(
        circular_files(&out).is_empty(),
        "an `import type` back edge has always been excluded — got: {:?}",
        circular_files(&out)
    );
}

// --- Mode A (envelope) parity -------------------------------------------------------------------
//
// The envelope lane builds `dep` by hand and cannot call the TypeScript resolver, so it used to fold
// its own copy of the exclusion verdict — two writers of one fact, the shape that drifts. Both lanes
// now only RECORD into `zzop_core::NoncycleCandidates` and the single `refine` decides, which is what
// these two tests pin: the same two shapes, through the other entry point.

use zzop_core::{
    FileProjection, ImportBinding, NormalizedEnvelope, SourceSymbol, SourceSymbolKind,
    NORMALIZED_AST_FORMAT,
};
use zzop_engine::analyze_envelope;

fn projection(path: &str) -> FileProjection {
    FileProjection {
        path: path.to_string(),
        loc: 10,
        ..Default::default()
    }
}

fn binds(file: &mut FileProjection, local: &str, specifier: &str, original: &str) {
    file.imports.insert(
        local.to_string(),
        ImportBinding {
            specifier: specifier.to_string(),
            original: original.to_string(),
            deferred: false,
            type_only: false,
        },
    );
}

fn declares(file: &mut FileProjection, name: &str, kind: SourceSymbolKind) {
    file.symbols.push(SourceSymbol {
        id: format!("{}#{name}", file.path),
        file: file.path.clone(),
        name: name.to_string(),
        kind,
        line: 1,
        exported: true,
        is_default: false,
        body_start: None,
        body_end: None,
        write_sites: Vec::new(),
    });
}

fn envelope_of(files: Vec<FileProjection>) -> NormalizedEnvelope {
    NormalizedEnvelope {
        format: NORMALIZED_AST_FORMAT.to_string(),
        version: zzop_core::NORMALIZED_AST_CONTRACT_VERSION.to_string(),
        parser: "fixture/1".to_string(),
        source: "fixture".to_string(),
        files,
    }
}

/// Lane 2 sees `ctrl.ts` import `State` from `svc.ts` BEFORE it has read `svc.ts`'s symbols — the
/// streaming order is exactly why the fold must be deferred rather than done per file.
#[test]
fn envelope_lane_excludes_a_value_spelled_import_of_an_exported_interface() {
    let mut ctrl = projection("a-ctrl.ts");
    binds(&mut ctrl, "State", "b-svc.ts", "State");
    declares(&mut ctrl, "Ctrl", SourceSymbolKind::Class);
    let mut svc = projection("b-svc.ts");
    binds(&mut svc, "Ctrl", "a-ctrl.ts", "Ctrl");
    declares(&mut svc, "State", SourceSymbolKind::Interface);

    let out = analyze_envelope(&envelope_of(vec![ctrl, svc]), &config());
    assert!(
        !out.findings.iter().any(|f| f.rule_id == "circular"),
        "the `State` edge is erased at compile time — no runtime cycle, got: {:?}",
        out.findings
            .iter()
            .filter(|f| f.rule_id == "circular")
            .collect::<Vec<_>>()
    );
}

/// The same control the native lane carries: a value declaration under the imported name keeps the
/// cycle, so the test above is the exclusion firing rather than the fixture failing to form a cycle.
#[test]
fn envelope_lane_keeps_the_cycle_when_the_export_is_a_value() {
    let mut ctrl = projection("a-ctrl.ts");
    binds(&mut ctrl, "state", "b-svc.ts", "state");
    declares(&mut ctrl, "Ctrl", SourceSymbolKind::Class);
    let mut svc = projection("b-svc.ts");
    binds(&mut svc, "Ctrl", "a-ctrl.ts", "Ctrl");
    declares(&mut svc, "state", SourceSymbolKind::Const);

    let out = analyze_envelope(&envelope_of(vec![ctrl, svc]), &config());
    assert!(
        out.findings.iter().any(|f| f.rule_id == "circular"),
        "`export const state` is a runtime value — the cycle must survive"
    );
}
