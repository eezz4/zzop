//! The CHANNEL half of the framework-recognizer seal — `FrameworkRecognizer::emits` against the io
//! each recognizer's own code actually constructs.
//!
//! # The hole this closes
//! `recognizer_drift` binds the MODULE axis (is every shipped recognizer declared, and does every
//! declaration have a module) in both directions. It says nothing about what a declared row CLAIMS to
//! emit. That axis had already produced a wrong answer with nothing to catch it: `hono` shipped
//! declaring only `io.consumes` while `router_mounts` fed its provides, and a HUMAN found it. A second
//! one — `spring security` declaring `io.provides` while constructing no io — was caught by this very
//! contract, pinned while the judgment was open, and CLOSED on 2026-08-02 by the fourth channel
//! (`channel::AUTH_EVIDENCE`): the row now declares the guard-evidence channel its code actually
//! feeds, and [`guard_evidence_types`] is the hop that derives it.
//!
//! # Why the code can answer this, and what the honest signal turned out to be
//! The open question was whether an adapter's RETURN TYPE (`Vec<IoProvide>` vs `Vec<IoConsume>`)
//! decides the channel. Measured across all eight parsers, it does not, twice over:
//!
//! - **A side is not a channel.** `channel::DB` is `"io.provides:db-table"` — a KIND, filled from both
//!   sides (`entity_decorators` provides a table, `typeorm_repository` consumes one, both are `DB`).
//!   So the classifier needs (side, kind), not side alone. Both are in the module's own source: the
//!   struct literal names the side and its `kind:` field names the kind. [`channel_of`] derives the
//!   split from `zzop_core`'s own constants rather than restating it — `DB` is recognized by
//!   RECOMPUTING the `"{PROVIDES}:{kind}"` spelling, so renaming the constant moves this with it.
//! - **Seven recognizers return no io at all.** `fastapi`, `django_routes`, `axum`, `gin`, `net_http` and
//!   typescript's `router_mounts` return `Vec<RouterMountFragment>`; `trpc_router` returns
//!   `Vec<ProcedureRouterFragment>`. The engine composes those into provides later. This is the exact
//!   shape that killed the earlier constructor-name sniffing attempt (it missed `next_pages_api` and
//!   under-detected, i.e. failed toward vacuous green). The fix is a hop rather than a heuristic:
//!   [`fragment_channels`] reads the ENGINE's own `compose_*` signatures — a fragment type in the
//!   parameters, `IoProvide`/`IoConsume` in the return — so the fragment→channel map is derived from
//!   the composition code and follows it automatically.
//! - **One channel is not io at all.** `channel::AUTH_EVIDENCE` is the decorator-guard side channel:
//!   its producers construct no `IoProvide`/`IoConsume` and no fragment, so both hops above are blind
//!   to them. [`guard_evidence_types`] is the same shape of hop one seam over: it reads the parser
//!   TYPES the engine's own callgraph guard gate names (`analyze/native_rules/callgraph/` —
//!   `SpringSecurityPosture`, `ForRoutesPattern`, `PythonGuardVocab`, `RustGuardVocab`), and a
//!   recognizer module whose code names such a type evidences the channel. Rust's `RustGuardVocab`
//!   lands in `lang/extractor_guards`, OUTSIDE the recognizer root — by design, matching
//!   `decorator_gate.rs`'s own judgment that rust guard evidence is a graph edge, not a side channel,
//!   so no rust row carries the channel and none should.
//!
//! # What is NOT bound (stated, because a guard's silence gets read as coverage)
//! - **A row with zero code evidence is not proven wrong, only unproven.** Those rows are pinned by
//!   name in [`CHANNEL_NOT_CODE_DERIVED`] with a reason each, so the set cannot grow silently — but a
//!   pin is a census, not a check.
//! - **Attribution granularity is the module, not the call site.** A module backing two rows lends its
//!   channels to both (`router_mounts` → `express` AND `hono`), so a row can inherit a channel a
//!   sibling vocabulary in the same module produced. Narrowing that needs per-row modules.
//! - **Kind literals are read per FILE, not per struct literal.** A file constructing both sides and
//!   naming two kinds would yield the cross product. No such file ships today; if one appears, this
//!   over-attributes rather than under-attributes, which is the safe direction for a guard.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use zzop_core::recognizer::{channel, FrameworkRecognizer};
use zzop_core::RULE_READ_IO_KINDS;

use crate::recognizer_drift::{recognizer_modules_by_crate, recognizer_root, NOT_A_FRAMEWORK};

/// The eight parser crates and their declarations, keyed by the DIRECTORY name
/// `recognizer_modules_by_crate` uses — the two halves have to meet somewhere, and a crate cannot be
/// enumerated from disk with its `const` linked in. `crate_declarations_cover_the_whole_aggregate`
/// below pins this list against `zzop_engine::framework_recognizers()` so it cannot silently drop one.
fn declarations_by_crate() -> Vec<(&'static str, &'static [FrameworkRecognizer])> {
    vec![
        (
            "parser-typescript",
            zzop_parser_typescript::FRAMEWORK_RECOGNIZERS,
        ),
        (
            "parser-python-3",
            zzop_parser_python_3::FRAMEWORK_RECOGNIZERS,
        ),
        ("parser-java-21", zzop_parser_java_21::FRAMEWORK_RECOGNIZERS),
        ("parser-csharp", zzop_parser_csharp::FRAMEWORK_RECOGNIZERS),
        ("parser-go", zzop_parser_go::FRAMEWORK_RECOGNIZERS),
        ("parser-rust", zzop_parser_rust::FRAMEWORK_RECOGNIZERS),
        ("parser-prisma", zzop_parser_prisma::FRAMEWORK_RECOGNIZERS),
        ("parser-sql", zzop_parser_sql::FRAMEWORK_RECOGNIZERS),
    ]
}

/// Rows whose channel set NO code evidence reaches, each with why. A pin is a census, not a check —
/// which is the whole reason it carries prose. EMPTY today: every row is code-derived.
///
/// Two rows have lived here and both came out exactly as this pin's contract directs (a row leaves
/// only by becoming derivable):
///
/// - `spring security` (until 2026-08-02) — a FINDING the contract itself made (the row declared
///   `io.provides` while constructing a `SpringSecurityPosture` and no io of any kind). The
///   `channel::AUTH_EVIDENCE` channel closed it: the row now declares what its code feeds and
///   [`guard_evidence_types`] derives it.
/// - `next.js` (until 2026-08-03) — a LIMIT of the mechanism: `next_pages_api`'s
///   `PagesApiHandlerScan` was turned into provides at a CALL site inside
///   `crates/engine/src/file_routes`, not through a typed `compose_*(fragment) -> IoProvide`
///   signature, so the type hop could not see it. Closed by standing that path up as the seam the
///   hop reads: the scan type moved to `zzop_core` (fragment convention) and the scan→provides step
///   became `analyze/compose/pages_api.rs`'s `compose_pages_api_provides(… zzop_core::
///   PagesApiHandlerScan …) -> Vec<IoProvide>` — behavior-unchanged, so [`fragment_channels`] now
///   derives the row from the composition code like every other fragment.
const CHANNEL_NOT_CODE_DERIVED: &[(&str, &str, &str)] = &[];

// ---------------------------------------------------------------------------------------------
// Source reading
// ---------------------------------------------------------------------------------------------

/// Comments removed, string literals KEPT (the `kind:` values are the evidence), and every
/// `#[cfg(test)]` ITEM dropped.
///
/// All three are load-bearing. `egress/consts.rs` names `zzop_core::ControllerPrefixRouteFragment` in a
/// doc comment ONLY — read as code that would lend `io.provides` to all six client rows in that module.
/// `class_shapes` names `IoProvide::body` the same way. And an inline `mod tests` re-spells its
/// module's literals on purpose, so counting it would make a fixture's vocabulary into a claim.
///
/// The test cut is per-ITEM, not "everything after the first `#[cfg(test)]`": eleven adapters declare
/// `#[cfg(test)] mod tests;` in the MIDDLE of the file, and truncating there silently blinded this
/// contract to the rest — `db_table_consume`'s only `kind: "db-table"` sits 67 lines below its
/// declaration, and the row read as unproven until the cut was narrowed.
pub(crate) fn code_only(text: &str) -> String {
    strip_cfg_test_items(&strip_comments(text))
}

/// Drop each `#[cfg(test)]`-attributed item: the braced body when it has one, the `mod tests;` line
/// when it does not.
fn strip_cfg_test_items(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut out: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if !lines[i].trim_start().starts_with("#[cfg(test)]") {
            out.push(lines[i]);
            i += 1;
            continue;
        }
        i += 1;
        let mut depth: i32 = 0;
        let mut opened = false;
        while i < lines.len() {
            let line = lines[i];
            depth += line.matches('{').count() as i32;
            depth -= line.matches('}').count() as i32;
            opened |= line.contains('{');
            i += 1;
            if opened && depth <= 0 {
                break;
            }
            if !opened && line.trim_end().ends_with(';') {
                break;
            }
        }
    }
    out.join("\n")
}

fn strip_comments(text: &str) -> String {
    let bytes: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        // Raw string: `r"..."`, `r#"..."#`, ... — copied verbatim, since a `//` inside one is data.
        if c == 'r' && bytes.get(i + 1).is_some_and(|n| *n == '"' || *n == '#') {
            let mut hashes = 0;
            let mut j = i + 1;
            while bytes.get(j) == Some(&'#') {
                hashes += 1;
                j += 1;
            }
            if bytes.get(j) == Some(&'"') {
                let close: Vec<char> = std::iter::once('"')
                    .chain(std::iter::repeat_n('#', hashes))
                    .collect();
                let end = (j + 1..=bytes.len().saturating_sub(close.len()))
                    .find(|k| bytes[*k..*k + close.len()] == close[..])
                    .map(|k| k + close.len())
                    .unwrap_or(bytes.len());
                out.extend(&bytes[i..end]);
                i = end;
                continue;
            }
        }
        if c == '"' {
            out.push(c);
            i += 1;
            while i < bytes.len() {
                out.push(bytes[i]);
                if bytes[i] == '\\' {
                    if let Some(n) = bytes.get(i + 1) {
                        out.push(*n);
                    }
                    i += 2;
                    continue;
                }
                if bytes[i] == '"' {
                    i += 1;
                    break;
                }
                i += 1;
            }
            continue;
        }
        if c == '/' && bytes.get(i + 1) == Some(&'/') {
            while i < bytes.len() && bytes[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if c == '/' && bytes.get(i + 1) == Some(&'*') {
            i += 2;
            while i < bytes.len() && !(bytes[i] == '*' && bytes.get(i + 1) == Some(&'/')) {
                i += 1;
            }
            i = (i + 2).min(bytes.len());
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

/// Every non-test `.rs` file under one module UNIT (`foo.rs` plus `foo/`), read and stripped.
pub(crate) fn unit_sources(root: &Path, unit: &str) -> Vec<String> {
    let mut out = Vec::new();
    let file = root.join(format!("{unit}.rs"));
    if file.is_file() {
        out.push(code_only(
            &std::fs::read_to_string(&file).unwrap_or_default(),
        ));
    }
    let mut stack = vec![root.join(unit)];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                if p.file_name().is_some_and(|n| n == "tests") {
                    continue;
                }
                stack.push(p);
                continue;
            }
            let name = p
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            if !name.ends_with(".rs") || name == "tests.rs" || name.ends_with("_tests.rs") {
                continue;
            }
            out.push(code_only(&std::fs::read_to_string(&p).unwrap_or_default()));
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Channel evidence
// ---------------------------------------------------------------------------------------------

/// (side, kind) -> channel, derived from `zzop_core`'s own constants.
///
/// The db channel is spelled `io.provides:db-table` — PROVIDES plus a kind — so it is recognized by
/// recomputing that composite instead of matching a `"db-table"` literal this file would then own a
/// second copy of. A kind no rule reads (the compose-phase sentinels `nest-global-prefix` and
/// `client-base-prefix`) is deliberately no channel at all: those are stripped before assembly
/// finishes, and counting them would credit `global_prefix`/`client_base` with provides/consumes they
/// never contribute to the join.
fn channel_of(provide_side: bool, kind: &str) -> Option<&'static str> {
    if !RULE_READ_IO_KINDS.contains(&kind) {
        return None;
    }
    if channel::DB == format!("{}:{kind}", channel::PROVIDES) {
        return Some(channel::DB);
    }
    Some(if provide_side {
        channel::PROVIDES
    } else {
        channel::CONSUMES
    })
}

/// Channels one source file's code evidences: the io sides it constructs crossed with the io kinds it
/// names. Both halves are required — `net_http.rs` names the STRING `"http"` as a fragment name while
/// constructing no io, and reading the kind alone would call that a provide.
fn channels_in(code: &str, out: &mut BTreeSet<&'static str>) {
    let sides: Vec<bool> = [("IoProvide", true), ("IoConsume", false)]
        .into_iter()
        .filter(|(ty, _)| code.contains(ty))
        .map(|(_, side)| side)
        .collect();
    for side in sides {
        for kind in RULE_READ_IO_KINDS {
            if code.contains(&format!("\"{kind}\"")) {
                out.extend(channel_of(side, kind));
            }
        }
    }
}

/// `zzop_core` fragment type -> the channel the engine composes it into, read off the engine's own
/// `compose_*` signatures (`crates/engine/src/analyze/compose/`).
///
/// Only PARAMETER types count: `compose_router_mount_provides` returns
/// `(Vec<IoProvide>, Vec<zzop_core::Attribute>)`, and letting the return side contribute would map
/// `Attribute` — a type half the workspace touches — onto `io.provides`.
fn fragment_channels(repo: &Path) -> BTreeMap<String, BTreeSet<&'static str>> {
    let dir = repo.join("crates/engine/src/analyze/compose");
    let mut out: BTreeMap<String, BTreeSet<&'static str>> = BTreeMap::new();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return out;
    };
    let mut units = BTreeSet::new();
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        units.insert(name.strip_suffix(".rs").unwrap_or(&name).to_string());
    }
    for unit in units {
        let sources = unit_sources(&dir, &unit);
        let mut channels = BTreeSet::new();
        for src in &sources {
            channels_in(src, &mut channels);
        }
        if channels.is_empty() {
            continue;
        }
        for src in &sources {
            for params in compose_param_lists(src) {
                for ty in core_types_in(&params) {
                    out.entry(ty).or_default().extend(channels.iter().copied());
                }
            }
        }
    }
    out
}

/// The parameter-list text of every `fn compose_…(` in one source, by paren matching.
fn compose_param_lists(code: &str) -> Vec<String> {
    const NEEDLE: &str = "fn compose_";
    let needle: Vec<char> = NEEDLE.chars().collect();
    let chars: Vec<char> = code.chars().collect();
    let mut out = Vec::new();
    let mut from = 0;
    while from + needle.len() <= chars.len() {
        let Some(start) = (from..=chars.len() - needle.len())
            .find(|k| chars[*k..*k + needle.len()] == needle[..])
        else {
            break;
        };
        from = start + needle.len();
        let Some(open) = chars[start..]
            .iter()
            .position(|c| *c == '(')
            .map(|p| start + p)
        else {
            break;
        };
        let mut depth = 0usize;
        let mut end = open;
        for (i, c) in chars.iter().enumerate().skip(open) {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = i;
                        break;
                    }
                }
                _ => {}
            }
        }
        if end > open {
            out.push(chars[open + 1..end].iter().collect());
        }
    }
    out
}

/// `zzop_core::Foo` identifiers named in a chunk of signature text.
fn core_types_in(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut from = 0;
    while let Some(rel) = text[from..].find("zzop_core::") {
        let start = from + rel + "zzop_core::".len();
        from = start;
        let ident: String = text[start..]
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if !ident.is_empty() {
            out.insert(ident);
        }
    }
    out
}

/// Parser crate (`parser-java-21`) -> the guard-evidence TYPES the engine's callgraph guard gate
/// consumes from it — the `channel::AUTH_EVIDENCE` analogue of [`fragment_channels`], read off the
/// engine's own code with the same honesty: one hop, no vocabulary owned here.
///
/// The subject is the callgraph native-rule unit (`crates/engine/src/analyze/native_rules/callgraph/`),
/// because that is where parser guard evidence crosses into the engine — `decorator_gate.rs`'s
/// `assemble_decorator_guarded` and its per-language feeder files. Every `zzop_parser_<crate>::Ident`
/// whose ident is TYPE-cased (uppercase initial) counts; function paths (`parse_calls`,
/// `extract_*`) are skipped because half of them are language-layer plumbing (`parse_imports`,
/// `resolve_file`) that would credit `lang` modules with a channel they do not fill. Unlike the
/// fragment hop this reads the whole unit rather than parameter lists only: the hazard that forced the
/// parameter restriction there (`Attribute`, a type half the workspace touches, on the return side)
/// has no analogue for parser-crate types, and the TS guard type (`ForRoutesPattern`) is named in a
/// let-binding, not a parameter. Over-attribution fails LOUD here (a module evidencing the channel
/// forces its row to declare it), which is the safe direction for a guard.
///
/// Stated residual: the express/hono guard words are NOT this channel and are deliberately absent —
/// they ride inside `RouterMountFragment`s and become `auth-guarded` ATTRIBUTES on those rows' own
/// `io.provides` at compose time (`compose/router_mounts.rs`), a compose-phase enrichment this
/// contract's io hop already covers, not a side channel.
fn guard_evidence_types(repo: &Path) -> BTreeMap<String, BTreeSet<String>> {
    let dir = repo.join("crates/engine/src/analyze/native_rules");
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for src in unit_sources(&dir, "callgraph") {
        let mut from = 0;
        while let Some(rel) = src[from..].find("zzop_parser_") {
            let start = from + rel + "zzop_parser_".len();
            from = start;
            let crate_ident: String = src[start..]
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            let Some(rest) = src[start + crate_ident.len()..].strip_prefix("::") else {
                continue;
            };
            let ident: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if ident.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
                out.entry(format!("parser-{}", crate_ident.replace('_', "-")))
                    .or_default()
                    .insert(ident);
            }
        }
    }
    out
}

/// Every recognizer module's evidenced channel set, keyed `parser-<lang>/<module>`.
fn module_channels(repo: &Path) -> BTreeMap<String, BTreeSet<&'static str>> {
    let fragments = fragment_channels(repo);
    let guard_types = guard_evidence_types(repo);
    let no_types = BTreeSet::new();
    let mut out = BTreeMap::new();
    for (krate, modules) in recognizer_modules_by_crate() {
        let root = recognizer_root(&repo.join("parser").join(&krate));
        let guards = guard_types.get(&krate).unwrap_or(&no_types);
        for module in modules {
            let sources = unit_sources(&root, &module);
            let mut channels = BTreeSet::new();
            for src in &sources {
                channels_in(src, &mut channels);
                for (ty, chans) in &fragments {
                    if src.contains(ty.as_str()) {
                        channels.extend(chans.iter().copied());
                    }
                }
                if guards.iter().any(|ty| src.contains(ty.as_str())) {
                    channels.insert(channel::AUTH_EVIDENCE);
                }
            }
            out.insert(format!("{krate}/{module}"), channels);
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Module -> row attribution
// ---------------------------------------------------------------------------------------------

fn fold(s: &str) -> String {
    s.to_ascii_lowercase()
        .replace(['-', '_', '/', '.', ' '], "")
}

/// Backtick-quoted tokens in an exemption reason — the row names a module states it backs. Matched
/// against the ACTUAL row vocabulary of the same crate, so this reads a closed set rather than mining
/// prose: a token that is not a declared framework name (`@RestController`, `parser-typescript`) is
/// simply not a row and drops out.
fn backticked(reason: &str) -> Vec<&str> {
    reason.split('`').skip(1).step_by(2).collect()
}

/// Which framework rows each module of one crate backs. Two sources, matching the two ways
/// `recognizer_drift` accounts for a module: an exemption states its rows in prose, and a
/// non-exempted module is bound to a row by the same name fold that file's direction 1 uses.
fn rows_by_module(krate: &str, rows: &[FrameworkRecognizer]) -> BTreeMap<String, BTreeSet<String>> {
    let names: BTreeSet<&str> = rows.iter().map(|r| r.framework).collect();
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (key, reason) in NOT_A_FRAMEWORK {
        let Some(module) = key.strip_prefix(&format!("{krate}/")) else {
            continue;
        };
        let named: BTreeSet<String> = backticked(reason)
            .into_iter()
            .filter(|t| names.contains(t))
            .map(str::to_string)
            .collect();
        out.insert(module.to_string(), named);
    }
    for module in recognizer_modules_by_crate()
        .remove(krate)
        .unwrap_or_default()
    {
        if out.contains_key(&module) {
            continue;
        }
        let m = fold(&module);
        // An EXACT fold match wins outright, substring containment is the fallback. The exact-first
        // notch is load-bearing because attribution (unlike "is it declared at all") propagates
        // evidence: `spring_security` folds to exactly the `spring security` row, and letting the
        // containment arm ALSO hand it to `spring` would force that row to declare the auth-evidence
        // channel its own modules never fill. (`recognizer_drift`'s direction 1 shared this predicate
        // until 2026-08-03 and now demands fold-EQUALITY plus an explicit alias map; the containment
        // arm survives here only for modules that file has already accounted for — today that is
        // `security`, whose MODULE_ROW_ALIASES entry over there names the same `spring security` row
        // this arm reaches — so a stray module cannot ride it into an attribution without first going
        // undeclared-red next door.)
        let exact: BTreeSet<String> = names
            .iter()
            .filter(|n| fold(n) == m)
            .map(|n| n.to_string())
            .collect();
        let matched: BTreeSet<String> = if exact.is_empty() {
            names
                .iter()
                .filter(|n| m.contains(&fold(n)) || fold(n).contains(&m))
                .map(|n| n.to_string())
                .collect()
        } else {
            exact
        };
        out.insert(module, matched);
    }
    out
}

fn repo_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

// ---------------------------------------------------------------------------------------------
// The floor (working-agreements §5.5) — a subject set smaller than the topic answers "covered"
// when it means "never looked at".
// ---------------------------------------------------------------------------------------------

/// The declaration lists this file enumerates by hand must BE the aggregate the engine publishes.
#[test]
fn crate_declarations_cover_the_whole_aggregate() {
    let by_crate = declarations_by_crate();
    assert!(
        by_crate.len() >= 8,
        "this workspace ships 8 parser crates; {} enumerated",
        by_crate.len()
    );
    let total: usize = by_crate.iter().map(|(_, l)| l.len()).sum();
    assert_eq!(
        total,
        zzop_engine::framework_recognizers().len(),
        "the per-crate lists here and zzop_engine::framework_recognizers() disagree — a crate was \
         added to one and not the other, and every channel check below would then skip it silently"
    );
    let on_disk = recognizer_modules_by_crate();
    for (krate, _) in &by_crate {
        assert!(
            on_disk.get(*krate).is_some_and(|m| !m.is_empty()),
            "{krate} contributes zero recognizer modules on disk — the directory name used here does \
             not match the one recognizer_modules_by_crate() produces, so this crate's rows would be \
             checked against an empty evidence set"
        );
    }
}

/// The evidence extractor itself must not be answering "nothing" everywhere. All four channels have
/// to appear, both derivation hops have to have found their maps, and the great majority of rows must
/// be backed — a green run on 2 evidenced rows and 30 unproven ones is exactly the vacuous state the
/// earlier name-sniffing attempt failed into.
#[test]
fn channel_evidence_is_not_vacuous() {
    let repo = repo_root();
    let fragments = fragment_channels(&repo);
    assert!(
        !fragments.is_empty(),
        "no compose_*(zzop_core::…Fragment) -> Io… signature found under \
         crates/engine/src/analyze/compose — the fragment hop is blind, and the seven recognizers \
         that return fragments instead of io would all read as unproven"
    );
    let guards = guard_evidence_types(&repo);
    for crate_name in ["parser-java-21", "parser-python-3", "parser-typescript"] {
        assert!(
            guards.contains_key(crate_name),
            "the callgraph guard gate no longer names any {crate_name} guard type (map: {:?}) — \
             that crate backs an auth-evidence row, so losing it means guard_evidence_types stopped \
             seeing the engine's own consumer and the row would read as an over-claim. (parser-rust \
             also appears in this map but backs no row — a bare `len() >= 3` floor was once satisfied \
             by it while a row-backing crate went missing, which is why this asserts membership, not \
             count.)",
            guards.keys().collect::<Vec<_>>()
        );
    }

    let modules = module_channels(&repo);
    let evidenced: Vec<&String> = modules
        .iter()
        .filter(|(_, c)| !c.is_empty())
        .map(|(m, _)| m)
        .collect();
    assert!(
        evidenced.len() >= 20,
        "only {} recognizer module(s) evidence any channel {:?} — the extractor stopped seeing code, \
         not the code stopped emitting io",
        evidenced.len(),
        evidenced
    );

    let seen: BTreeSet<&&str> = modules.values().flatten().collect();
    for c in [
        channel::PROVIDES,
        channel::CONSUMES,
        channel::DB,
        channel::AUTH_EVIDENCE,
    ] {
        assert!(
            seen.contains(&c),
            "no module anywhere evidences {c:?} — a quarter of the channel vocabulary is invisible to \
             this contract, so every row declaring it is unchecked"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// The contract
// ---------------------------------------------------------------------------------------------

/// THE BINDING — a row's declared `emits` equals the channels its backing modules construct.
///
/// Union per framework NAME within a crate, because a framework legitimately ships as several rows
/// (`hono` has one per side) and as several modules (`hono`'s provides live in `router_mounts`, its
/// consumes in `hono_client`). Both directions fail here: a declared channel no module produces is an
/// over-claim, a produced channel no row declares is the `hono` bug.
#[test]
fn every_row_declares_the_channels_its_modules_actually_construct() {
    let repo = repo_root();
    let modules = module_channels(&repo);
    let pinned: BTreeSet<(&str, &str)> = CHANNEL_NOT_CODE_DERIVED
        .iter()
        .map(|(k, f, _)| (*k, *f))
        .collect();

    let mut offenders = Vec::new();
    for (krate, rows) in declarations_by_crate() {
        let attribution = rows_by_module(krate, rows);
        let mut declared: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
        for r in rows {
            declared
                .entry(r.framework)
                .or_default()
                .extend(r.emits.iter().copied());
        }
        for (framework, declared) in declared {
            let mut evidenced: BTreeSet<&str> = BTreeSet::new();
            let mut backing = Vec::new();
            for (module, names) in &attribution {
                if !names.contains(framework) {
                    continue;
                }
                backing.push(module.clone());
                if let Some(c) = modules.get(&format!("{krate}/{module}")) {
                    evidenced.extend(c.iter().copied());
                }
            }
            if evidenced.is_empty() {
                if !pinned.contains(&(krate, framework)) {
                    offenders.push(format!(
                        "{krate} row {framework:?} declares {declared:?} but its module(s) {backing:?} \
                         construct no io — either the row over-claims, or the composition path is one \
                         this contract cannot follow (then pin it in CHANNEL_NOT_CODE_DERIVED with a \
                         reason)"
                    ));
                }
                continue;
            }
            if declared != evidenced {
                offenders.push(format!(
                    "{krate} row {framework:?} declares {declared:?} but its module(s) {backing:?} \
                     construct {evidenced:?} (missing from the declaration: {:?}; declared with no \
                     code behind it: {:?})",
                    evidenced.difference(&declared).collect::<Vec<_>>(),
                    declared.difference(&evidenced).collect::<Vec<_>>(),
                ));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "FrameworkRecognizer::emits disagrees with the io its recognizers build:\n{}",
        offenders.join("\n")
    );
}

/// No module builds io that no row discloses. The check above walks rows; a module attributed to
/// NOTHING is invisible to it, which is how a new adapter could ship a whole channel undeclared.
#[test]
fn every_module_that_builds_io_is_attributed_to_a_declared_row() {
    let repo = repo_root();
    let modules = module_channels(&repo);
    let mut orphans = Vec::new();
    for (krate, rows) in declarations_by_crate() {
        for (module, names) in rows_by_module(krate, rows) {
            let key = format!("{krate}/{module}");
            let channels = modules.get(&key).cloned().unwrap_or_default();
            if !channels.is_empty() && names.is_empty() {
                orphans.push(format!(
                    "{key} builds {channels:?} but backs no declared row"
                ));
            }
        }
    }
    assert!(
        orphans.is_empty(),
        "recognizer module(s) building io that no FrameworkRecognizer row claims:\n{}\n\
         Name the row in that module's NOT_A_FRAMEWORK reason (backticked), or declare it.",
        orphans.join("\n")
    );
}

/// The pin cannot outlive what it excuses. An entry whose row became code-derivable, or whose row is
/// gone, would silently keep that row exempt from the binding above.
#[test]
fn no_pin_names_a_row_that_is_now_derivable_or_absent() {
    let repo = repo_root();
    let modules = module_channels(&repo);
    let mut stale = Vec::new();
    for &(krate, framework, reason) in CHANNEL_NOT_CODE_DERIVED {
        assert!(
            reason.len() > 40,
            "{krate}/{framework}'s pin reason is too thin to be a judgment: {reason:?}"
        );
        let Some((_, rows)) = declarations_by_crate()
            .into_iter()
            .find(|(k, _)| *k == krate)
        else {
            stale.push(format!("{krate} is not a parser crate"));
            continue;
        };
        if !rows.iter().any(|r| r.framework == framework) {
            stale.push(format!("{krate} declares no row {framework:?}"));
            continue;
        }
        let evidenced = rows_by_module(krate, rows)
            .into_iter()
            .filter(|(_, names)| names.contains(framework))
            .any(|(m, _)| {
                modules
                    .get(&format!("{krate}/{m}"))
                    .is_some_and(|c| !c.is_empty())
            });
        if evidenced {
            stale.push(format!(
                "{krate} row {framework:?} now HAS code evidence — delete the pin so the binding checks it"
            ));
        }
    }
    assert!(
        stale.is_empty(),
        "CHANNEL_NOT_CODE_DERIVED is stale:\n{}",
        stale.join("\n")
    );
}
