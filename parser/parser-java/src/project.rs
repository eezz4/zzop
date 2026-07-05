//! Project-wide (whole-corpus) Spring HTTP route PROVIDES resolution — a superset of
//! `provides::extract_http_provides`'s per-file pass, resolving two facts invisible to a single file:
//!
//! 1. **Non-literal class-level prefixes**: `@RequestMapping(Path.ASSET_PATH)` where `Path.ASSET_PATH` is a
//!    `public static final String` in another file, possibly a `+`-concatenation of further constants. A
//!    large codebase can declare many same-named package-private constants, so resolution is SCOPED TO THE
//!    DECLARING CLASS'S OWN `extends` chain, not name-global (see `resolve_scoped`) — an unresolvable term
//!    is counted, never guessed.
//! 2. **CE-style base-class routing**: `FooController extends FooControllerCE`, where the real mapping
//!    methods and prefix live on `FooControllerCE` (no `@RestController` of its own) while `FooController`
//!    — usually a different file — carries `@RestController` but declares no methods. Spring discovers
//!    mapping annotations by walking a *bean's* full class hierarchy, so this is a real route, resolved
//!    here via a corpus-wide `extends`-chain walk (`walk_chain`).
//!
//! Both facts are whole-project, not per-file, so wiring them into the incremental per-file cache would
//! leave other cached files' provides silently stale on an edit — this module is a standalone whole-corpus
//! pass callers run explicitly instead.
//!
//! ## Determinism
//! An unresolvable prefix or ambiguous class name SKIPS every route needing it (never a wrong/empty
//! prefix) — see `ProjectProvidesReport::skipped_unresolved_prefix` / `skipped_ambiguous_class_name`.
//!
//! ## Known limits (v1 scope)
//! Resolution is by SIMPLE class name only (no package/import qualification); a nested class's own fields
//! are not excluded from its enclosing class's constant scan; interfaces are not walked in `extends` chains.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use regex::Regex;
use zpz_core::{http_interface_key, IoProvide, SourceSymbol, SourceSymbolKind};

use crate::parse_method_spans;
use crate::provides::{class_annotation_facts, enclosing_class, first_quoted_string, method_route};

/// Result of a whole-corpus `extract_http_provides_project` run — see module doc's "Determinism" section
/// for what each skip counter means.
#[derive(Debug, Clone, Default)]
pub struct ProjectProvidesReport {
    pub provides: Vec<IoProvide>,
    /// Classes whose routes could not be emitted because a class-level `@RequestMapping` in their
    /// `extends` chain referenced a constant, or a qualifier/`extends` class, that did not resolve.
    pub skipped_unresolved_prefix: u32,
    /// Simple class names declared in 2+ files in this corpus (cannot safely tell which declaration an
    /// `extends`/qualifier reference means) — a count of the individual duplicate declarations skipped.
    pub skipped_ambiguous_class_name: u32,
}

/// One class's structural facts from a single-file lexical scan — like `provides::class_context`, but
/// also keeps the RAW `@RequestMapping` argument, the `extends` target, and this class's own
/// `static final String` constants, which the per-file fast path doesn't need.
struct ClassRow {
    file: String,
    symbol: SourceSymbol,
    extends: Option<String>,
    is_controller: bool,
    request_mapping_arg: Option<String>,
    /// This class's own `static final String NAME = <raw RHS expression>;` fields (not yet evaluated) —
    /// see `resolve_declared`/`eval_concat_expr` for how a raw expression becomes a literal value.
    constants: HashMap<String, String>,
}

/// A class-level `@RequestMapping` prefix's resolution state — see module doc's "Determinism" section.
enum PrefixState {
    /// No `@RequestMapping` on this class at all — contributes no prefix, but does not block a search for
    /// one further down the chain.
    NoMapping,
    Resolved(String),
    /// An `@RequestMapping` is present but its argument did not resolve to a literal value.
    Unresolved,
}

/// Extracts Spring MVC HTTP route `IoProvide`s across an entire Java corpus — see module doc for what this
/// resolves beyond `provides::extract_http_provides`'s per-file pass. Never panics: built entirely on
/// `parse_method_spans` plus plain string/regex scans, same as the per-file pass this reuses.
pub fn extract_http_provides_project(files: &[(String, String)]) -> ProjectProvidesReport {
    let mut per_file: HashMap<&str, (Vec<&str>, Vec<SourceSymbol>)> = HashMap::new();
    let mut rows_by_name: HashMap<String, Vec<ClassRow>> = HashMap::new();

    for (rel, text) in files {
        let symbols = parse_method_spans(rel, text);
        let lines: Vec<&str> = text.lines().collect();
        for class in symbols.iter().filter(|s| s.kind == SourceSymbolKind::Class) {
            let facts = class_annotation_facts(class, &lines);
            let header = class_header_text(&lines, class.line);
            let extends = extends_re().captures(&header).map(|c| c[1].to_string());
            let is_interface = interface_re().is_match(&header);
            let body_start = class.body_start.unwrap_or(class.line);
            let body_end = class.body_end.unwrap_or(body_start);
            let constants = class_own_constants(&lines, body_start, body_end, is_interface);
            rows_by_name
                .entry(class.name.clone())
                .or_default()
                .push(ClassRow {
                    file: rel.clone(),
                    symbol: class.clone(),
                    extends,
                    is_controller: facts.is_controller,
                    request_mapping_arg: facts.request_mapping_arg,
                    constants,
                });
        }
        per_file.insert(rel.as_str(), (lines, symbols));
    }

    // Unique-name resolution: a simple class name declared in exactly one file is safe to use as an
    // `extends`/qualifier resolution target; 2+ declarations (a common name reused across the corpus) is
    // ambiguous.
    let mut skipped_ambiguous_class_name = 0u32;
    let mut classes: HashMap<String, ClassRow> = HashMap::new();
    for (name, mut rows) in rows_by_name {
        if rows.len() == 1 {
            classes.insert(name, rows.pop().unwrap());
        } else {
            skipped_ambiguous_class_name += rows.len() as u32;
        }
    }

    let mut provides = Vec::new();
    let mut seen: HashSet<(String, String, u32, Option<String>)> = HashSet::new();
    let mut skipped_unresolved_prefix = 0u32;
    let mut memo: HashMap<(String, String), Option<String>> = HashMap::new();

    // Every class carrying its own @RestController/@Controller is a gating root — walk its `extends`
    // chain (this class, its superclass, its superclass's superclass, ...), emitting routes for every
    // class along the way (module doc point 2). Root names are collected up front and sorted so iteration
    // order — and therefore `provides`' order before any caller-side sort — is deterministic regardless of
    // `HashMap` iteration order.
    let mut root_names: Vec<&String> = classes
        .iter()
        .filter(|(_, row)| row.is_controller)
        .map(|(name, _)| name)
        .collect();
    root_names.sort();

    for root_name in root_names {
        walk_chain(
            root_name,
            &classes,
            &per_file,
            &mut memo,
            &mut provides,
            &mut seen,
            &mut skipped_unresolved_prefix,
        );
    }

    ProjectProvidesReport {
        provides,
        skipped_unresolved_prefix,
        skipped_ambiguous_class_name,
    }
}

/// Walks `root_name`'s `extends` chain (root first, then its superclass, ...; capped and cycle-safe) and
/// emits every class's own routes along the way: the first class in root-to-leaf order that declares its
/// own `@RequestMapping` (resolved or not) fixes the prefix — or the unresolved-skip — for every class
/// from that point on.
#[allow(clippy::too_many_arguments)]
fn walk_chain(
    root_name: &str,
    classes: &HashMap<String, ClassRow>,
    per_file: &HashMap<&str, (Vec<&str>, Vec<SourceSymbol>)>,
    memo: &mut HashMap<(String, String), Option<String>>,
    provides: &mut Vec<IoProvide>,
    seen: &mut HashSet<(String, String, u32, Option<String>)>,
    skipped_unresolved_prefix: &mut u32,
) {
    let mut chain_names: Vec<String> = Vec::new();
    let mut visited: HashSet<String> = HashSet::new();
    let mut cur = Some(root_name.to_string());
    let mut depth = 0;
    while let Some(name) = cur {
        // Cycle or pathologically deep chain (never expected in real inheritance) — defensive, not a panic.
        if depth > 40 || !visited.insert(name.clone()) {
            break;
        }
        depth += 1;
        let next = classes.get(&name).and_then(|c| c.extends.clone());
        chain_names.push(name);
        cur = next;
    }

    let mut effective: Option<String> = None;
    let mut blocked = false;
    for name in &chain_names {
        let Some(row) = classes.get(name) else {
            continue;
        };
        if effective.is_none() && !blocked {
            match resolve_class_prefix(name, &row.request_mapping_arg, classes, memo) {
                PrefixState::NoMapping => {}
                PrefixState::Resolved(p) => effective = Some(p),
                PrefixState::Unresolved => blocked = true,
            }
        }
        if blocked {
            *skipped_unresolved_prefix += 1;
            continue;
        }
        let prefix = effective.clone().unwrap_or_default();
        emit_class_routes(row, &prefix, per_file, provides, seen);
    }
}

/// Emits every `IoProvide` for `row`'s OWN methods (not a nested class's, gated via `SourceSymbol::id`)
/// under `prefix`, deduped against `seen` so two different gating roots reaching the same base class with
/// the same resolved prefix (the `FooController extends FooControllerCE` shape) do not double-emit.
fn emit_class_routes(
    row: &ClassRow,
    prefix: &str,
    per_file: &HashMap<&str, (Vec<&str>, Vec<SourceSymbol>)>,
    provides: &mut Vec<IoProvide>,
    seen: &mut HashSet<(String, String, u32, Option<String>)>,
) {
    let Some((lines, symbols)) = per_file.get(row.file.as_str()) else {
        return;
    };
    let file_classes: Vec<&SourceSymbol> = symbols
        .iter()
        .filter(|s| s.kind == SourceSymbolKind::Class)
        .collect();
    for method in symbols
        .iter()
        .filter(|s| s.kind == SourceSymbolKind::Function)
    {
        let Some(cls) = enclosing_class(method, &file_classes) else {
            continue;
        };
        if cls.id != row.symbol.id {
            continue; // belongs to a different (possibly nested) class in the same file.
        }
        let Some((verb, path)) = method_route(lines, method.line) else {
            continue;
        };
        let full_path = format!("{prefix}/{path}");
        let key = http_interface_key(&verb, &full_path);
        let dedupe_key = (
            key.clone(),
            row.file.clone(),
            method.line,
            Some(method.name.clone()),
        );
        if !seen.insert(dedupe_key) {
            continue;
        }
        provides.push(IoProvide {
            kind: "http".to_string(),
            key,
            file: row.file.clone(),
            line: method.line,
            symbol: Some(method.name.clone()),
        });
    }
}

/// The class/interface declaration's own header text, from `class_line` (1-indexed) up to and including the
/// first line containing `{`, capped at 20 lines. Used to find both the `extends` target and whether this
/// is an `interface` declaration (see `class_own_constants`).
fn class_header_text(lines: &[&str], class_line: u32) -> String {
    let mut text = String::new();
    let mut idx = class_line.saturating_sub(1) as usize;
    let mut scanned = 0;
    while idx < lines.len() && scanned < 20 {
        let line = lines[idx];
        text.push_str(line);
        text.push('\n');
        if line.contains('{') {
            break;
        }
        idx += 1;
        scanned += 1;
    }
    text
}

/// This class's own `static final String NAME = <raw RHS>;` declarations, scanned from its body span
/// (`body_start`..`body_end`, 1-indexed inclusive). `<raw RHS>` is kept as unevaluated text (a bare literal
/// or a `+`-concatenation expression — see `eval_concat_expr`). `is_interface` relaxes the modifier
/// requirement: an interface field (`String APPLICATIONS = "...";` inside `interface Entity { ... }`) is
/// implicitly `public static final` with no modifier keywords written at all, which `constant_decl_re`'s
/// explicit `static`+`final` requirement would otherwise miss. A plain class's fields do NOT get this
/// relaxation — a same-shaped field there could be a genuine mutable instance field, not a constant.
fn class_own_constants(
    lines: &[&str],
    body_start: u32,
    body_end: u32,
    is_interface: bool,
) -> HashMap<String, String> {
    let start = body_start.saturating_sub(1) as usize;
    let end = (body_end as usize).min(lines.len());
    let mut out = HashMap::new();
    if start >= end {
        return out;
    }
    let text = lines[start..end].join("\n");
    for cap in constant_decl_re().captures_iter(&text) {
        out.insert(cap[1].to_string(), cap[2].trim().to_string());
    }
    if is_interface {
        for cap in interface_field_re().captures_iter(&text) {
            out.entry(cap[1].to_string())
                .or_insert_with(|| cap[2].trim().to_string());
        }
    }
    out
}

/// Resolves one class-level `@RequestMapping`'s raw argument text to a `PrefixState` — a literal quoted
/// string resolves directly; a bare/empty argument list resolves to `""`; anything else is treated as a
/// (possibly qualified) constant reference and resolved via `resolve_scoped`, starting at `class_name`
/// (used only for an unqualified reference — a qualified one starts at its own qualifier instead).
fn resolve_class_prefix(
    class_name: &str,
    raw: &Option<String>,
    classes: &HashMap<String, ClassRow>,
    memo: &mut HashMap<(String, String), Option<String>>,
) -> PrefixState {
    let Some(args) = raw else {
        return PrefixState::NoMapping;
    };
    if let Some(lit) = first_quoted_string(args) {
        return PrefixState::Resolved(lit);
    }
    if args.trim().is_empty() {
        return PrefixState::Resolved(String::new());
    }
    let Some((qualifier, name)) = const_ref_qualified(args) else {
        return PrefixState::Unresolved;
    };
    let mut visiting = HashSet::new();
    match resolve_scoped(
        class_name,
        qualifier.as_deref(),
        &name,
        classes,
        memo,
        &mut visiting,
    ) {
        Some(v) => PrefixState::Resolved(v),
        None => PrefixState::Unresolved,
    }
}

/// Resolves a possibly-qualified constant reference to its literal value. `qualifier` is `None` for a bare
/// reference — resolved by walking `scope_class`'s OWN `extends` chain (real Java field scoping). A dotted
/// reference (`Path.ASSET_PATH`) resolves by walking the QUALIFIER'S chain instead, ignoring `scope_class`
/// (a qualified reference is never scope-relative in Java). `None` if the starting class isn't in `classes`
/// (ambiguous or undeclared) or no class in the chain declares `const_name`.
fn resolve_scoped(
    scope_class: &str,
    qualifier: Option<&str>,
    const_name: &str,
    classes: &HashMap<String, ClassRow>,
    memo: &mut HashMap<(String, String), Option<String>>,
    visiting: &mut HashSet<(String, String)>,
) -> Option<String> {
    let start = qualifier.unwrap_or(scope_class);
    let mut cur = Some(start.to_string());
    let mut walked = HashSet::new();
    let mut depth = 0;
    while let Some(cname) = cur {
        if depth > 40 || !walked.insert(cname.clone()) {
            return None; // cycle or pathological depth — defensive, not expected in real inheritance.
        }
        depth += 1;
        let row = classes.get(&cname)?;
        if let Some(raw) = row.constants.get(const_name) {
            return resolve_declared(&cname, const_name, raw, classes, memo, visiting);
        }
        cur = row.extends.clone();
    }
    None
}

/// Resolves one already-located declaration, memoizing into `memo` (keyed by the ACTUAL declaring class,
/// so two reference paths landing on the same declaration share the cache) and guarding against a
/// reference cycle via `visiting` (defensive against a lexical misparse; real `static final` fields cannot
/// actually be cyclic).
fn resolve_declared(
    owner_class: &str,
    const_name: &str,
    raw: &str,
    classes: &HashMap<String, ClassRow>,
    memo: &mut HashMap<(String, String), Option<String>>,
    visiting: &mut HashSet<(String, String)>,
) -> Option<String> {
    let key = (owner_class.to_string(), const_name.to_string());
    if let Some(v) = memo.get(&key) {
        return v.clone();
    }
    if !visiting.insert(key.clone()) {
        return None;
    }
    let value = eval_concat_expr(owner_class, raw, classes, memo, visiting);
    visiting.remove(&key);
    memo.insert(key, value.clone());
    value
}

/// Evaluates a `+`-concatenation expression (`BASE_PATH + VERSION + "/assets"`), owned by `owner_class`, to
/// a literal String term by term — a quoted term is literal, an identifier term resolves via
/// `resolve_scoped`. `None` if any term fails to resolve.
fn eval_concat_expr(
    owner_class: &str,
    expr: &str,
    classes: &HashMap<String, ClassRow>,
    memo: &mut HashMap<(String, String), Option<String>>,
    visiting: &mut HashSet<(String, String)>,
) -> Option<String> {
    let mut out = String::new();
    for term in split_plus_terms(expr) {
        let term = term.trim();
        if let Some(lit) = quoted_literal(term) {
            out.push_str(&lit);
            continue;
        }
        let (qualifier, name) = split_qualified(term)?;
        out.push_str(&resolve_scoped(
            owner_class,
            qualifier.as_deref(),
            &name,
            classes,
            memo,
            visiting,
        )?);
    }
    Some(out)
}

/// Splits a `+`-concatenation expression into its terms, honoring `"..."` quoting so a `+` inside a string
/// literal is never treated as a term separator (defensive; unlikely in a route-prefix constant).
fn split_plus_terms(expr: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let mut cur = String::new();
    let mut in_str = false;
    for c in expr.chars() {
        match c {
            '"' => {
                in_str = !in_str;
                cur.push(c);
            }
            '+' if !in_str => terms.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    if !cur.trim().is_empty() {
        terms.push(cur);
    }
    terms
}

fn quoted_literal(term: &str) -> Option<String> {
    if term.len() >= 2 && term.starts_with('"') && term.ends_with('"') {
        Some(term[1..term.len() - 1].to_string())
    } else {
        None
    }
}

/// Splits an identifier term into `(qualifier, name)` — `Entity.APPLICATIONS` -> `(Some("Entity"),
/// "APPLICATIONS")`, `BASE_PATH` -> `(None, "BASE_PATH")`. For a multi-segment reference, only the LAST
/// segment before the final name is the qualifier (this module's simple-name model). `None` if `term` is
/// not identifier-shaped at all.
fn split_qualified(term: &str) -> Option<(Option<String>, String)> {
    let c = qualified_ident_re().captures(term)?;
    let qualifier = c.get(1).map(|m| m.as_str().to_string());
    Some((qualifier, c[2].to_string()))
}

/// The (optional, dotted-or-bare) constant reference found anywhere in an annotation's raw argument text —
/// `Path.ASSET_PATH` -> `(Some("Path"), "ASSET_PATH")`, bare `ASSET_PATH` -> `(None, "ASSET_PATH")`. Restricted to
/// a `SCREAMING_SNAKE_CASE` final name (Java constant convention) so an attribute name like `value`
/// (lowercase) is never mistaken for a constant reference; the qualifier itself is unrestricted (a class
/// name is not necessarily all-caps).
fn const_ref_qualified(args: &str) -> Option<(Option<String>, String)> {
    let c = const_ref_re().captures(args)?;
    let qualifier = c.get(1).map(|m| m.as_str().to_string());
    Some((qualifier, c[2].to_string()))
}

fn extends_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\bextends\s+([A-Za-z_$][\w$]*)").unwrap())
}

fn interface_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\binterface\s").unwrap())
}

/// A Java interface field with NO modifier keywords at all (implicitly `public static final` — see
/// `class_own_constants`). An interface field written WITH explicit modifiers is still covered by
/// `constant_decl_re`; both regexes run for an interface body and `or_insert` keeps whichever matches.
fn interface_field_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?m)^\s*String\s+([A-Z][A-Z0-9_]*)\s*=\s*([^;]*);").unwrap())
}

fn qualified_ident_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^(?:(?:[A-Za-z_$][\w$]*\.)*([A-Za-z_$][\w$]*)\.)?([A-Za-z_$][\w$]*)$").unwrap()
    })
}

fn const_ref_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b(?:([A-Za-z_][A-Za-z0-9_]*)\.)?([A-Z][A-Z0-9_]*)\b").unwrap())
}

fn constant_decl_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?m)^\s*(?:(?:public|private|protected)\s+)?(?:static\s+final|final\s+static)\s+String\s+([A-Z][A-Z0-9_]*)\s*=\s*([^;]*);",
        )
        .unwrap()
    })
}

#[cfg(test)]
mod tests {
    //! Coverage for `extract_http_provides_project`: cross-file literal `@RequestMapping` (sanity — same
    //! shape the per-file pass already covers, just via two files), cross-file constant-reference
    //! resolution (the `Path.ASSET_PATH` shape), a `+`-concatenated constant expression, class-scoped
    //! resolution surviving an unrelated same-named constant elsewhere in the corpus, the
    //! ambiguous-qualifier-class skip, the CE-split class-hierarchy gate (methods on an un-annotated base
    //! class reached only via a `@RestController` subclass in another file), and the full
    //! `ResourceController`/`ResourceControllerCE`/`Path`/`PathCE` cross-file shape end to end.
    use super::*;

    fn keys(report: &ProjectProvidesReport) -> Vec<String> {
        let mut v: Vec<String> = report.provides.iter().map(|p| p.key.clone()).collect();
        v.sort();
        v
    }

    #[test]
    fn cross_file_literal_prefix_still_resolves() {
        let files = vec![
            (
                "C.java".to_string(),
                "@RestController\n@RequestMapping(\"/x\")\nclass C {\n  @GetMapping(\"/y\")\n  void y() {}\n}\n"
                    .to_string(),
            ),
            ("Other.java".to_string(), "class Unrelated {}\n".to_string()),
        ];
        let report = extract_http_provides_project(&files);
        assert_eq!(keys(&report), vec!["GET /x/y"]);
        assert_eq!(report.skipped_unresolved_prefix, 0);
    }

    #[test]
    fn cross_file_constant_reference_prefix_resolves() {
        let files = vec![
            (
                "Path.java".to_string(),
                "class Path {\n  public static final String ASSET_PATH = \"/asset\";\n}\n".to_string(),
            ),
            (
                "ResourceController.java".to_string(),
                "@RestController\n@RequestMapping(Path.ASSET_PATH)\nclass ResourceController {\n  @GetMapping(\"/{id}\")\n  void get() {}\n}\n"
                    .to_string(),
            ),
        ];
        let report = extract_http_provides_project(&files);
        assert_eq!(keys(&report), vec!["GET /asset/{}"]);
        assert_eq!(report.skipped_unresolved_prefix, 0);
    }

    #[test]
    fn concatenated_constant_expression_resolves_recursively() {
        let files = vec![
            (
                "Path.java".to_string(),
                "class Path {\n    static final String BASE_PATH = \"/api\";\n    static final String VERSION = \"/v1\";\n    public static final String ASSET_PATH = BASE_PATH + VERSION + \"/assets\";\n}\n"
                    .to_string(),
            ),
            (
                "ResourceController.java".to_string(),
                "@RestController\n@RequestMapping(Path.ASSET_PATH)\nclass ResourceController {\n  @GetMapping(\"/{id}\")\n  void get() {}\n}\n"
                    .to_string(),
            ),
        ];
        let report = extract_http_provides_project(&files);
        assert_eq!(keys(&report), vec!["GET /api/v1/assets/{}"]);
        assert_eq!(report.skipped_unresolved_prefix, 0);
    }

    #[test]
    fn class_scoped_resolution_survives_an_unrelated_same_named_constant_elsewhere() {
        // `Path`'s own `BASE_PATH` must resolve even though a completely unrelated class elsewhere in the
        // corpus also declares a `BASE_PATH` constant with a different value — a corpus-global-unique-by-name
        // model would wrongly treat that as ambiguous and break every route prefix.
        let files = vec![
            (
                "Path.java".to_string(),
                "class Path {\n    static final String BASE_PATH = \"/api\";\n    static final String VERSION = \"/v1\";\n    public static final String ASSET_PATH = BASE_PATH + VERSION + \"/assets\";\n}\n"
                    .to_string(),
            ),
            (
                "unrelated/SomeService.java".to_string(),
                "class SomeService {\n    private static final String BASE_PATH = \"https://example.com/\";\n}\n"
                    .to_string(),
            ),
            (
                "ResourceController.java".to_string(),
                "@RestController\n@RequestMapping(Path.ASSET_PATH)\nclass ResourceController {\n  @GetMapping(\"/{id}\")\n  void get() {}\n}\n"
                    .to_string(),
            ),
        ];
        let report = extract_http_provides_project(&files);
        assert_eq!(keys(&report), vec!["GET /api/v1/assets/{}"]);
        assert_eq!(report.skipped_unresolved_prefix, 0);
    }

    #[test]
    fn ambiguous_qualifier_class_name_is_skipped_not_guessed() {
        let files = vec![
            (
                "a/Path.java".to_string(),
                "class Path {\n  public static final String ASSET_PATH = \"/asset\";\n}\n".to_string(),
            ),
            (
                "b/Path.java".to_string(),
                "class Path {\n  public static final String ASSET_PATH = \"/other-asset\";\n}\n".to_string(),
            ),
            (
                "ResourceController.java".to_string(),
                "@RestController\n@RequestMapping(Path.ASSET_PATH)\nclass ResourceController {\n  @GetMapping(\"/{id}\")\n  void get() {}\n}\n"
                    .to_string(),
            ),
        ];
        let report = extract_http_provides_project(&files);
        assert!(
            report.provides.is_empty(),
            "an ambiguous qualifier class must never guess a prefix, got: {:?}",
            report.provides
        );
        assert_eq!(report.skipped_unresolved_prefix, 1);
        assert_eq!(report.skipped_ambiguous_class_name, 2);
    }

    #[test]
    fn ce_split_base_class_routes_are_reached_through_a_restcontroller_subclass_in_another_file() {
        let files = vec![
            (
                "ce/ResourceControllerCE.java".to_string(),
                "@RequestMapping(\"/asset\")\nclass ResourceControllerCE {\n  @GetMapping(\"/{id}\")\n  void getById() {}\n}\n"
                    .to_string(),
            ),
            (
                "ResourceController.java".to_string(),
                "@RestController\n@RequestMapping(\"/asset\")\nclass ResourceController extends ResourceControllerCE {\n}\n"
                    .to_string(),
            ),
        ];
        let report = extract_http_provides_project(&files);
        assert_eq!(keys(&report), vec!["GET /asset/{}"]);
        let p = &report.provides[0];
        assert_eq!(p.file, "ce/ResourceControllerCE.java");
        assert_eq!(p.symbol.as_deref(), Some("getById"));
    }

    #[test]
    fn a_base_class_with_no_restcontroller_descendant_anywhere_emits_nothing() {
        let files = vec![(
            "ce/OrphanCE.java".to_string(),
            "@RequestMapping(\"/orphan\")\nclass OrphanCE {\n  @GetMapping(\"/x\")\n  void x() {}\n}\n"
                .to_string(),
        )];
        let report = extract_http_provides_project(&files);
        assert!(report.provides.is_empty());
    }

    #[test]
    fn interface_constant_with_no_modifier_keywords_still_resolves() {
        // `Entity.APPLICATIONS`: `Entity` is an INTERFACE whose fields are implicitly `public static
        // final` with no modifier keywords written at all.
        let files = vec![
            (
                "Entity.java".to_string(),
                "public interface Entity {\n    String APPLICATIONS = \"applications\";\n}\n"
                    .to_string(),
            ),
            (
                "Path.java".to_string(),
                "class Path {\n    static final String BASE_PATH = \"/api\";\n    public static final String APPLICATION_PATH = BASE_PATH + \"/\" + Entity.APPLICATIONS;\n}\n"
                    .to_string(),
            ),
            (
                "ApplicationController.java".to_string(),
                "@RestController\n@RequestMapping(Path.APPLICATION_PATH)\nclass ApplicationController {\n  @GetMapping(\"/{id}\")\n  void get() {}\n}\n"
                    .to_string(),
            ),
        ];
        let report = extract_http_provides_project(&files);
        assert_eq!(keys(&report), vec!["GET /api/applications/{}"]);
        assert_eq!(report.skipped_unresolved_prefix, 0);
    }

    #[test]
    fn cross_file_base_class_and_constant_resolution_end_to_end() {
        let files = vec![
            (
                "constants/ce/PathCE.java".to_string(),
                "package com.example.app.constants.ce;\n\npublic class PathCE {\n    static final String BASE_PATH = \"/api\";\n    static final String VERSION = \"/v1\";\n    public static final String ASSET_PATH = BASE_PATH + VERSION + \"/assets\";\n}\n".to_string(),
            ),
            (
                "constants/Path.java".to_string(),
                "package com.example.app.constants;\n\nimport com.example.app.constants.ce.PathCE;\n\npublic class Path extends PathCE {}\n".to_string(),
            ),
            (
                "controllers/ce/ResourceControllerCE.java".to_string(),
                "package com.example.app.controllers.ce;\n\nimport com.example.app.constants.Path;\nimport org.springframework.web.bind.annotation.GetMapping;\nimport org.springframework.web.bind.annotation.RequestMapping;\n\n@RequestMapping(Path.ASSET_PATH)\npublic class ResourceControllerCE {\n\n    @GetMapping(\"/{id}\")\n    public void getById() {}\n}\n".to_string(),
            ),
            (
                "controllers/ResourceController.java".to_string(),
                "package com.example.app.controllers;\n\nimport com.example.app.constants.Path;\nimport com.example.app.controllers.ce.ResourceControllerCE;\nimport org.springframework.web.bind.annotation.RequestMapping;\nimport org.springframework.web.bind.annotation.RestController;\n\n@RestController\n@RequestMapping(Path.ASSET_PATH)\npublic class ResourceController extends ResourceControllerCE {\n}\n".to_string(),
            ),
        ];
        let report = extract_http_provides_project(&files);
        assert_eq!(keys(&report), vec!["GET /api/v1/assets/{}"]);
        assert_eq!(report.skipped_unresolved_prefix, 0);
        assert_eq!(report.skipped_ambiguous_class_name, 0);
    }
}
