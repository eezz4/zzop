use super::*;

// A default vocabulary: these projections read only `rust_guard()` out of it, and the Rust arm is not
// what any assertion here is about.
fn vocab() -> crate::vocabulary::VocabularyConfig {
    crate::vocabulary::VocabularyConfig::default()
}

const CONTROLLER: &str = r#"
import { Controller, Get, UseGuards } from '@nestjs/common';
import { AuthGuard } from './auth';

@Controller('users')
@UseGuards(AuthGuard)
export class UsersController {
  @Get()
  list() {
    return collect();
  }
}
"#;

#[test]
fn a_typescript_file_yields_its_own_call_sites() {
    let facts = project(
        Some(Language::TypeScript),
        false,
        "a.ts",
        CONTROLLER,
        &vocab().resolve(),
    );
    assert!(
        facts.raw_calls.iter().any(|c| c.callee_name == "collect"),
        "the call inside `list` should be attributed to it: {:?}",
        facts.raw_calls
    );
}

#[test]
fn the_nest_guard_evidence_rides_the_same_parse() {
    let facts = project(
        Some(Language::TypeScript),
        false,
        "a.ts",
        CONTROLLER,
        &vocab().resolve(),
    );
    assert!(
        !facts.controller_guarded_lines.is_empty(),
        "`@UseGuards` on the controller should mark its route lines: {facts:?}"
    );
}

// The gate is a real parse, not a language name. A degraded file used to contribute nothing to the
// call graph because the pass never reached it; that must stay true now that the projection is early.
#[test]
fn a_degraded_parse_contributes_nothing() {
    let facts = project(
        Some(Language::TypeScript),
        true,
        "a.ts",
        CONTROLLER,
        &vocab().resolve(),
    );
    assert!(facts.is_empty(), "{facts:?}");
}

#[test]
fn a_non_typescript_language_contributes_nothing() {
    let facts = project(
        Some(Language::Python),
        false,
        "a.py",
        "def f():\n    g()\n",
        &vocab().resolve(),
    );
    assert!(facts.is_empty(), "{facts:?}");
}

// `.svelte` and `.vue` ride the TypeScript dispatch slot for their imports, but reading their whole
// file as TypeScript reads template markup as code. The pass's own loop skipped them by extension and
// this projection has to skip them the same way — a difference here would change what the graph holds.
#[test]
fn an_overlay_file_in_the_typescript_slot_is_skipped_by_extension() {
    let facts = project(
        Some(Language::TypeScript),
        false,
        "a.svelte",
        "<script>export function f() { g(); }</script>",
        &vocab().resolve(),
    );
    assert!(facts.is_empty(), "{facts:?}");
}

#[test]
fn a_file_with_nothing_to_say_is_empty_rather_than_four_empty_containers() {
    let facts = project(
        Some(Language::TypeScript),
        false,
        "a.ts",
        "export const x = 1;\n",
        &vocab().resolve(),
    );
    assert!(facts.is_empty(), "{facts:?}");
}
