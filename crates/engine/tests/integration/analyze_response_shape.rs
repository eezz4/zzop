//! End-to-end coverage for `response-shape-v1` + its 1st consuming rule
//! `cross-layer/sensitive-response-field`, wired from `zzop_engine::analyze_trees` over real
//! TypeScript: the Nest controller adapter captures each handler's DECLARED return type
//! (`Promise<X>` unwrapped), assemble resolves the name against the tree-wide class/interface shape
//! merge, and the rule flags declared response fields whose NAMES are sensitive-shaped — escalating
//! to critical when the cross-layer join proves the route is actually consumed.
//!
//! Four handler shapes in one BE fixture pin the whole contract:
//! - `getMe(): Promise<AccountDto>` (passwordHash field) + an FE `fetch` of the route -> CRITICAL.
//! - `getSession(): Promise<SessionDto>` (token field), consumed by nothing -> WARNING.
//! - `getProfile(): Promise<ProfileDto>` (clean interface) -> silent (and pins interface resolution).
//! - `getLegacy()` (NO return type, body returns the same sensitive data at runtime) -> silent
//!   facts + the per-tree "declare a return type" disclosure — the honesty half: an undeclared
//!   handler is never guessed at AND never silently absent from the run.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_engine::{analyze_trees, EngineConfig};

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

/// Declares the sensitive-response vocabulary explicitly: `VocabularyConfig::default()` is the empty
/// declaration ("make none of these judgments"), so a fixture that declared nothing would test a run
/// equivalent to a user who never ran `init` — same rationale as
/// `analyze_be_framework_coverage_warning.rs`'s config helper. The values are the rule crate's own
/// shipped defaults (what the `init` template writes).
fn config(source_id: &str) -> EngineConfig {
    fn owned(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_string()).collect()
    }
    let mut cfg = EngineConfig {
        source_id: source_id.to_string(),
        ..EngineConfig::default()
    };
    cfg.vocabulary.sensitive_response_field_substrings =
        owned(zzop_rules_cross_layer::SENSITIVE_RESPONSE_FIELD_SUBSTRINGS);
    cfg.vocabulary.sensitive_response_field_exact_names =
        owned(zzop_rules_cross_layer::SENSITIVE_RESPONSE_FIELD_EXACT);
    cfg.vocabulary.sensitive_response_field_suffixes =
        owned(zzop_rules_cross_layer::SENSITIVE_RESPONSE_FIELD_SUFFIXES);
    cfg
}

fn find<'a>(findings: &'a [zzop_core::Finding], rule_id: &str) -> Vec<&'a zzop_core::Finding> {
    findings.iter().filter(|f| f.rule_id == rule_id).collect()
}

/// BE tree: one Nest controller, DTOs in a SEPARATE file (the cross-file resolution this fact
/// exists for), one clean INTERFACE referent, one undeclared handler.
fn be_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-response-be");
    dir.write(
        "src/account.controller.ts",
        "import { AccountDto, SessionDto } from './dtos';\n\
         import { ProfileDto } from './profile';\n\
         @Controller('account')\n\
         export class AccountController {\n\
         \x20 @Get('me')\n\
         \x20 getMe(): Promise<AccountDto> { return this.svc.me(); }\n\
         \x20 @Get('profile')\n\
         \x20 getProfile(): Promise<ProfileDto> { return this.svc.profile(); }\n\
         \x20 @Get('session')\n\
         \x20 getSession(): Promise<SessionDto> { return this.svc.session(); }\n\
         \x20 @Get('legacy')\n\
         \x20 getLegacy() { return this.svc.me(); }\n\
         }\n",
    );
    dir.write(
        "src/dtos.ts",
        "export class AccountDto {\n\
         \x20 id: string;\n\
         \x20 email: string;\n\
         \x20 passwordHash: string;\n\
         }\n\
         export class SessionDto {\n\
         \x20 id: string;\n\
         \x20 token: string;\n\
         }\n",
    );
    dir.write(
        "src/profile.ts",
        "export interface ProfileDto {\n\
         \x20 id: string;\n\
         \x20 displayName?: string;\n\
         }\n",
    );
    dir
}

/// FE tree: consumes exactly `GET /account/me` — the join proves the sensitive route is live.
fn fe_tree() -> TempDir {
    let dir = TempDir::new("zzop-engine-response-fe");
    dir.write(
        "src/api.ts",
        "export function loadMe() { return fetch('/account/me'); }\n",
    );
    dir
}

#[test]
fn sensitive_response_field_fires_warning_and_escalates_to_critical_when_consumed() {
    let be = be_tree();
    let fe = fe_tree();
    let trees = vec![
        (be.path().to_path_buf(), config("be")),
        (fe.path().to_path_buf(), config("fe")),
    ];
    let out = analyze_trees(&trees);

    // The join must resolve the FE call to the sensitive route — the escalation's evidence.
    assert!(
        out.cross_layer
            .edges
            .iter()
            .any(|e| e.key == "GET /account/me"),
        "expected the FE fetch to join the provider route: {:?}",
        out.cross_layer.edges
    );

    let flagged = find(
        &out.cross_layer_findings,
        "cross-layer/sensitive-response-field",
    );
    assert_eq!(
        flagged.len(),
        2,
        "exactly the two sensitive-DTO routes fire (clean interface + undeclared stay silent): {flagged:?}"
    );

    let me = flagged
        .iter()
        .find(|f| f.message.contains("GET /account/me"))
        .expect("the consumed route fires");
    assert_eq!(
        me.severity,
        zzop_core::Severity::Critical,
        "a consumed route escalates: {me:?}"
    );
    assert_eq!(me.file, "src/account.controller.ts");
    assert!(
        me.message.contains("passwordHash"),
        "names the field: {}",
        me.message
    );
    assert!(
        me.message.contains("consumed"),
        "states the escalation evidence: {}",
        me.message
    );
    let data = me.data.as_ref().expect("finding carries data");
    assert_eq!(data["source"], "be");
    assert_eq!(data["consumed"], true);
    assert_eq!(data["sensitiveFields"], serde_json::json!(["passwordHash"]));

    let session = flagged
        .iter()
        .find(|f| f.message.contains("GET /account/session"))
        .expect("the unconsumed sensitive route fires too (provide-side only)");
    assert_eq!(
        session.severity,
        zzop_core::Severity::Warning,
        "no witnessed consumer -> stays warning: {session:?}"
    );
    assert!(
        session.message.contains("token"),
        "names the field: {}",
        session.message
    );

    // The clean-interface route and the undeclared route never fire.
    assert!(
        !flagged
            .iter()
            .any(|f| f.message.contains("/account/profile")),
        "clean declared shape must stay silent: {flagged:?}"
    );
    assert!(
        !flagged
            .iter()
            .any(|f| f.message.contains("/account/legacy")),
        "an undeclared handler is never guessed at: {flagged:?}"
    );
}

/// An interface GETTER signature is an own readable property (`interface R { get password(): string }`
/// is satisfied by `{ password: "…" }`, and the `@Expose()`-getter idiom makes it wire surface) — so a
/// declared response naming such an interface must fire the sensitive-field rule. Before 2026-08-03 the
/// member filter dropped it (request-DTO logic applied to both directions): silent FN + `complete: true`.
#[test]
fn interface_getter_member_is_response_surface_and_fires_the_sensitive_field_rule() {
    let be = TempDir::new("zzop-engine-response-getter");
    be.write(
        "src/session.controller.ts",
        "import { GetterSessionView } from './view';\n\
         @Controller('session')\n\
         export class SessionController {\n\
         \x20 @Get('view')\n\
         \x20 getView(): Promise<GetterSessionView> { return this.svc.view(); }\n\
         }\n",
    );
    be.write(
        "src/view.ts",
        "export interface GetterSessionView {\n\
         \x20 id: string;\n\
         \x20 get password(): string;\n\
         }\n",
    );
    let trees = vec![(be.path().to_path_buf(), config("be"))];
    let out = analyze_trees(&trees);
    let flagged = find(
        &out.cross_layer_findings,
        "cross-layer/sensitive-response-field",
    );
    assert_eq!(flagged.len(), 1, "{:?}", out.cross_layer_findings);
    assert_eq!(flagged[0].severity, zzop_core::Severity::Warning);
    assert!(
        flagged[0].message.contains("password"),
        "names the getter-declared field: {}",
        flagged[0].message
    );
}

/// DIRECTION-COUPLING MEASUREMENT (commissioned with the getter capture): `ShapeMerge` serves BOTH
/// resolutions, so an interface getter captured for the response direction ALSO reaches
/// `IoProvide::body` when the same interface types a `@Body()` param — this pin records that
/// measured fact. Judgment on why this is NOT a request-direction misread (so no direction split is
/// needed today): a TS interface getter member is a required READABLE property — an FE object
/// literal for that type must carry the key (`{}` fails the type-check against `GetterSessionView`),
/// so a body-field-drift "required field never set" over it claims exactly what the type system
/// claims. If a real-world FP class emerges here (e.g. widespread getter-only view interfaces
/// reused as body types), this pin is where the direction split starts: it documents today's
/// coupling as a choice, not an accident.
#[test]
fn interface_getter_reaches_the_body_direction_as_a_required_field_measured_and_judged() {
    let be = TempDir::new("zzop-engine-body-getter");
    be.write(
        "src/session.controller.ts",
        "import { GetterSessionView } from './view';\n\
         @Controller('session')\n\
         export class SessionController {\n\
         \x20 @Post('save')\n\
         \x20 save(@Body() dto: GetterSessionView): Promise<GetterSessionView> { return this.svc.save(dto); }\n\
         }\n",
    );
    be.write(
        "src/view.ts",
        "export interface GetterSessionView {\n\
         \x20 id: string;\n\
         \x20 get password(): string;\n\
         }\n",
    );
    let trees = vec![(be.path().to_path_buf(), config("be"))];
    let out = analyze_trees(&trees);
    let (_, _, be_out) = &out.trees[0];
    let io = be_out.ir.ir.io.as_ref().expect("assembled io");
    let provide = io
        .provides
        .iter()
        .find(|p| p.key == "POST /session/save")
        .expect("the @Post route provides");
    let body = provide.body.as_ref().expect("resolved @Body() shape");
    let password = body
        .fields
        .iter()
        .find(|f| f.name == "password")
        .expect("the getter member reaches the body direction (measured coupling — see doc)");
    assert!(
        !password.optional,
        "a getter member is a required readable property: {body:?}"
    );
    assert!(body.complete, "{body:?}");
}

/// The capture-less disclosure, end to end (the ⓖ-residual gap this closes): a 100% Express-style
/// tree — router mounts, no Nest capture anywhere — used to produce ZERO response findings AND zero
/// disclosures (its `.ts` extension sits inside the sightline cover, so no blind-spot row either),
/// indistinguishable from a clean tree. Now ONE per-tree warning names the capture-less route count
/// with its own advice (NOT the sentinel's "declare a return type" — declaring does not turn the
/// analysis on for these shapes).
#[test]
fn express_style_tree_gets_the_capture_less_disclosure() {
    let be = TempDir::new("zzop-engine-response-express");
    be.write(
        "src/routes.ts",
        "export function getSession(req, res) { res.json({ id: '1', passwordHash: 'x' }); }\n\
         apiRoutes.get('/session', getSession);\n\
         apiRoutes.post('/session', getSession);\n",
    );
    let mut cfg = config("be");
    cfg.vocabulary.router_names = vec!["apiRoutes".to_string()];
    let trees = vec![(be.path().to_path_buf(), cfg)];
    let out = analyze_trees(&trees);
    let (_, _, be_out) = &out.trees[0];
    let capture: Vec<&String> = be_out
        .warnings
        .iter()
        .filter(|w| w.contains("no response-shape evidence"))
        .collect();
    assert_eq!(capture.len(), 1, "{:?}", be_out.warnings);
    let w = capture[0];
    assert!(w.contains("2 of 2 http routes"), "{w}");
    assert!(w.contains("src/routes.ts"), "{w}");
    assert!(
        w.contains("declaring a return type alone does not turn it on there"),
        "{w}"
    );
    assert!(
        !be_out
            .warnings
            .iter()
            .any(|w| w.contains("declare no return type")),
        "no Nest sentinel exists here — the sentinel disclosure must stay silent: {:?}",
        be_out.warnings
    );
}

/// The negative: a pure-Nest tree whose every route carries a captured response gets NO capture-less
/// disclosure — the warning is about routes the capture cannot reach, not about the tree's framework.
#[test]
fn pure_nest_tree_with_captured_routes_gets_no_capture_less_disclosure() {
    let be = TempDir::new("zzop-engine-response-purenest");
    be.write(
        "src/account.controller.ts",
        "import { AccountDto } from './dtos';\n\
         @Controller('account')\n\
         export class AccountController {\n\
         \x20 @Get('me')\n\
         \x20 getMe(): Promise<AccountDto> { return this.svc.me(); }\n\
         }\n",
    );
    be.write(
        "src/dtos.ts",
        "export class AccountDto {\n\x20 id: string;\n}\n",
    );
    let trees = vec![(be.path().to_path_buf(), config("be"))];
    let out = analyze_trees(&trees);
    let (_, _, be_out) = &out.trees[0];
    assert!(
        !be_out
            .warnings
            .iter()
            .any(|w| w.contains("no response-shape evidence")),
        "every route is captured — nothing to disclose: {:?}",
        be_out.warnings
    );
}

/// Mixed tree, exact counts, all three disclosures disjoint: one captured Nest route (no
/// disclosure), one undeclared Nest handler (sentinel disclosure), one Nest route with an
/// UNCAPTURABLE annotation (`Promise<AccountDto[]>` — never-guess None, the wire value that makes
/// the None=capture-less derivation false) and two router-mount routes — the capture-less warning
/// counts 3 of 5.
#[test]
fn mixed_tree_counts_capture_less_routes_exactly() {
    let be = TempDir::new("zzop-engine-response-mixed");
    be.write(
        "src/account.controller.ts",
        "import { AccountDto } from './dtos';\n\
         @Controller('account')\n\
         export class AccountController {\n\
         \x20 @Get('me')\n\
         \x20 getMe(): Promise<AccountDto> { return this.svc.me(); }\n\
         \x20 @Get('all')\n\
         \x20 getAll(): Promise<AccountDto[]> { return this.svc.all(); }\n\
         \x20 @Get('legacy')\n\
         \x20 getLegacy() { return this.svc.me(); }\n\
         }\n",
    );
    be.write(
        "src/dtos.ts",
        "export class AccountDto {\n\x20 id: string;\n}\n",
    );
    be.write(
        "src/routes.ts",
        "export function h(req, res) { res.json({}); }\n\
         apiRoutes.get('/legacy-a', h);\n\
         apiRoutes.post('/legacy-b', h);\n",
    );
    let mut cfg = config("be");
    cfg.vocabulary.router_names = vec!["apiRoutes".to_string()];
    let trees = vec![(be.path().to_path_buf(), cfg)];
    let out = analyze_trees(&trees);
    let (_, _, be_out) = &out.trees[0];
    let capture = be_out
        .warnings
        .iter()
        .find(|w| w.contains("no response-shape evidence"))
        .expect("capture-less disclosure present");
    assert!(capture.contains("3 of 5 http routes"), "{capture}");
    assert!(
        be_out
            .warnings
            .iter()
            .any(|w| w.contains("1 route handler") && w.contains("declare no return type")),
        "the undeclared handler keeps the sentinel disclosure: {:?}",
        be_out.warnings
    );
}

/// The honesty half of "no declaration = silence + guidance": the undeclared handler produces no fact and no
/// finding, and the OWNING tree's warnings disclose it with actionable wording — so "0 response
/// findings" on an annotation-free tree is distinguishable from a clean one.
#[test]
fn undeclared_return_type_is_disclosed_on_the_owning_trees_warnings() {
    let be = be_tree();
    let trees = vec![(be.path().to_path_buf(), config("be"))];
    let out = analyze_trees(&trees);

    let (_, source, be_out) = &out.trees[0];
    assert_eq!(source, "be");
    let disclosure: Vec<&String> = be_out
        .warnings
        .iter()
        .filter(|w| w.contains("declare no return type"))
        .collect();
    assert_eq!(
        disclosure.len(),
        1,
        "ONE aggregated disclosure per tree: {:?}",
        be_out.warnings
    );
    let w = disclosure[0];
    assert!(w.contains("1 route handler"), "{w}");
    assert!(w.contains("src/account.controller.ts"), "{w}");
    assert!(w.contains("Promise<SomeDto>"), "{w}");
    assert!(
        w.contains("sensitive-response-field"),
        "the disclosure names what turning it on enables: {w}"
    );
}

/// Declaration-removal control (the "removing the declaration silences it" half of the never-guess pin): byte-identical
/// tree except `getMe` loses its return-type annotation — the critical finding disappears entirely
/// (no guess from the handler body), while the same run still discloses the now-undeclared handlers.
#[test]
fn removing_the_declaration_silences_the_finding_never_guessed() {
    let be = TempDir::new("zzop-engine-response-be-undeclared");
    be.write(
        "src/account.controller.ts",
        "import { AccountDto } from './dtos';\n\
         @Controller('account')\n\
         export class AccountController {\n\
         \x20 @Get('me')\n\
         \x20 getMe() { return this.svc.me(); }\n\
         }\n",
    );
    be.write(
        "src/dtos.ts",
        "export class AccountDto {\n\
         \x20 id: string;\n\
         \x20 passwordHash: string;\n\
         }\n",
    );
    let fe = fe_tree();
    let trees = vec![
        (be.path().to_path_buf(), config("be")),
        (fe.path().to_path_buf(), config("fe")),
    ];
    let out = analyze_trees(&trees);

    assert!(
        out.cross_layer
            .edges
            .iter()
            .any(|e| e.key == "GET /account/me"),
        "the join still resolves — only the response FACT is absent: {:?}",
        out.cross_layer.edges
    );
    let flagged = find(
        &out.cross_layer_findings,
        "cross-layer/sensitive-response-field",
    );
    assert!(
        flagged.is_empty(),
        "no declaration -> no fact -> no finding, even though the runtime body returns the same DTO: {flagged:?}"
    );
    let (_, _, be_out) = &out.trees[0];
    assert!(
        be_out
            .warnings
            .iter()
            .any(|w| w.contains("declare no return type")),
        "the silence is disclosed, never swallowed: {:?}",
        be_out.warnings
    );
}
