//! Contract 11: reference validation — a shipped message must never recommend a config key/flag that does
//! not exist. This is the machine contract for the defect class a message audit found live: `--since=all`,
//! `--repo=`, and `scanners.vocabulary.commitTypePatterns` were all recommended by real messages despite
//! none of them being real knobs. Both checks below load
//! `crates/config/config-surface.json` — the single vocabulary file also consumed by the Rust config
//! front-end (`crates/config`)'s unknown-key drift warnings, so the config front-end's own runtime and
//! this test can never disagree about what a valid flag/config key is.
//!
//! The vocabulary structs and the extraction/validation helpers live in `config_surface.rs`.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::collect_rs_files;
use crate::config_surface::{
    extract_config_context_tokens, extract_flag_references, load_config_surface,
    unknown_config_context_tokens, unknown_flag_references,
};

/// A workspace-relative directory, resolved from this test crate's manifest dir (`crates/engine`, since
/// `rule_contracts` lives in `crates/engine/tests/`).
fn ws_dir(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel)
}

/// The glob a [`scan_roots`] entry expands to, for failure messages.
fn root_glob(rel: &str) -> String {
    format!("{rel}/**/*.rs")
}

/// Every Cargo workspace member, read out of the root `Cargo.toml`'s `[workspace].members` array —
/// the SHIPPED list of packages this repo builds, and the thing [`scan_roots`] derives its scan set
/// from.
///
/// Extraction is textual (this test crate has no TOML parser) but fails loudly rather than quietly:
/// a missing `members = [` anchor or an unclosed array panics instead of returning a short list, and
/// [`the_derived_scan_set_covers_every_package_in_the_workspace`] reconciles the parsed result
/// against the packages actually on disk, so a parse that silently loses entries cannot narrow the
/// census.
///
/// COMMENTS ARE CUT FIRST, per line, and that is not a nicety — this workspace's `members` array is
/// half prose, and the first draft of this parse searched the raw text for the closing `]`, found the
/// one inside `# -> zzop[.exe]`, and returned 8 of the 23 members. The whole packages/, parser/ and
/// rules/ half of the workspace would have gone unscanned while every remaining root still contributed
/// files. That miss is the reason [`the_derived_scan_set_covers_every_package_in_the_workspace`]
/// exists at all: a derivation is not automatically two-sided just because it stopped being a literal.
fn workspace_members() -> Vec<String> {
    let path = ws_dir("Cargo.toml");
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
    const ANCHOR: &str = "members = [";
    let start = text.find(ANCHOR).unwrap_or_else(|| {
        panic!(
            "no `{ANCHOR}` array in {} — this census derives its scan roots from the workspace \
             member list; re-point this extraction rather than letting it read nothing",
            path.display()
        )
    });
    let quoted = regex::Regex::new("\"([^\"]+)\"").expect("static regex");
    let mut out = Vec::new();
    let mut closed = false;
    for line in text[start + ANCHOR.len()..].lines() {
        let code = line.split('#').next().unwrap_or_default();
        out.extend(quoted.captures_iter(code).map(|c| c[1].to_string()));
        if code.contains(']') {
            closed = true;
            break;
        }
    }
    assert!(
        closed,
        "the `{ANCHOR}` array in {} is never closed by a `]` outside a comment — the parse ran off \
         the end of the file and would have swept every later quoted string in as a member",
        path.display()
    );
    out
}

/// Files inside a scan root that are deliberately subtracted from the census, each with the reason.
///
/// **This is the one thing here that can rot into a hole, so it is kept as close to empty as the truth
/// allows** — every entry is a file where a shipped message could name a knob that does not exist and
/// nothing would say so. An exclusion list that grows is how this repo kills a guard (the reason
/// `check-vendor-token-literals.sh` never got an escape hatch), so add an entry only with a reason
/// that survives being read back a year later.
/// [`the_derived_scan_set_covers_every_package_in_the_workspace`] fails when an entry stops naming a
/// file the census would otherwise hold — a stale exclusion is itself a silent widening.
///
/// The single entry earns itself twice over. `crates/git/src/process.rs` is not a message surface at
/// all: it is the argv of an external `git` invocation (that crate makes exactly two
/// `std::process::Command` calls, and the file's own module doc says so), so its `--pretty`,
/// `--no-merges`, `--reverse`, `--numstat`, `--date` and `--since` are git's flags spelled to RUN git,
/// not recommendations made to a zzop user. The alternative — vouching all six in
/// `config-surface.json`'s `externalToolFlags` — was rejected because it would vouch **`--since`**
/// globally, and `--since=all` is one of the two shipped defects (`reference_unit_tests.rs` pins it)
/// this whole contract was built to catch. A scope subtraction on one argv builder is strictly
/// narrower than a vocabulary entry that would blind every root at once.
const UNSCANNED_FILES: [(&str, &str); 1] = [(
    "crates/git/src/process.rs",
    "external `git` argv, not user-facing prose — vouching its flags instead would vouch `--since` \
     and reopen the `--since=all` defect class",
)];

/// THE scan-root list for contract 11's two real-tree checks — every workspace member's `src`
/// directory, DERIVED from [`workspace_members`] rather than hand-listed.
///
/// # Why derived (2026-07-28)
///
/// This used to be a literal array of 8 directories, and the literal went stale twice before anyone
/// noticed — `crates/config` was added 2026-07-17 and `packages/mcp` + `packages/cli-bin` 2026-07-23,
/// each time AFTER user-facing messages had already shipped unscanned in them. The array was still
/// stale on the day it was replaced: `crates/git/src` and `crates/cache/src` had never been in it, and
/// planting the historical real defect string `--since=all` (a flag `crates/git` is exactly the crate
/// to recommend) in `crates/git/src/lib.rs` left the whole suite GREEN. A hand list of "which of our
/// own crates ship messages" is a fact about the workspace, and the workspace manifest already owns it.
///
/// The former per-root prose is gone with the array, and deliberately: a paragraph justifying why each
/// of eight directories deserves scanning was answering a question nobody should have to ask. The
/// default is now "every package we build", and the only thing that needs a written reason is an
/// EXCLUSION (see [`UNSCANNED_FILES`]).
///
/// Note this also narrows one root: `rules/native` used to be walked as a directory OF CRATES
/// (`rules/native/*/src/**/*.rs`, a bespoke second walk mode). Each of those four crates is its own
/// workspace member, so they now arrive as four ordinary `<member>/src` roots — same files, no special
/// case, and a fifth native rule crate joins the census by being a member instead of by matching a glob.
///
/// # `crates/*/tests` is DELIBERATELY outside this set (2026-07-28)
///
/// Every root ends in `/src`, and that is a decision rather than an oversight — which is worth
/// stating, because it has a real consequence: **a file that moves from `src/` into `tests/` leaves
/// this census silently.** It happened (`crates/summary/tests/host_dispatch.rs`), so whoever moves a
/// file next should know the move costs coverage here.
///
/// The scan is NOT widened, and the reason is what a wider scan would actually catch. This contract
/// asks "does every config-key-shaped token in SHIPPED prose name a real key?" A test file's strings
/// are mostly deliberate fakes — a bogus key written precisely to prove the unknown-key walk warns
/// about it. Widening would flag those by design, an allowlist would grow to hold them, and **a guard
/// whose allowlist grows stops guarding anything** (the reason `check-vendor-token-literals.sh` never
/// got an escape hatch). Measured before deciding: zero live violations sit in `tests/` today.
///
/// The asymmetry against `surface_parity`'s runtime keyset pin is intentional and worth naming: there,
/// the thing at risk is a REPLY a user reads, and a silently dropped field is invisible. Here it is a
/// string a test wrote about itself.
fn scan_roots() -> Vec<String> {
    let mut out: Vec<String> = workspace_members()
        .into_iter()
        .map(|member| format!("{member}/src"))
        .collect();
    out.sort();
    out.dedup();
    out
}

/// The canonicalized [`UNSCANNED_FILES`] paths. A declared exclusion that does not resolve is a hard
/// failure rather than a silent skip — the same contract `cli_only_lane_sources` in the sibling
/// `surface_parity.rs` holds its registry to.
fn unscanned_files() -> BTreeSet<PathBuf> {
    UNSCANNED_FILES
        .iter()
        .map(|(rel, reason)| {
            fs::canonicalize(ws_dir(rel)).unwrap_or_else(|e| {
                panic!(
                    "`UNSCANNED_FILES` excludes {rel} ({reason}), which does not resolve ({e}) — a \
                     moved or deleted file must drop its exclusion in the SAME commit, or the \
                     exclusion silently starts covering nothing"
                )
            })
        })
        .collect()
}

/// The `.rs` files one [`scan_roots`] entry contributes. Degrades to an empty `Vec` on a missing
/// directory (`collect_rs_files`/`read_dir` both do), which is exactly what
/// [`every_declared_scan_root_actually_contributes_files_to_the_census`] exists to catch.
fn collect_root(rel: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect_rs_files(&ws_dir(rel), &mut out);
    out
}

/// The full scanned-file set for contract 11's two real-tree checks — every [`scan_roots`] entry's
/// files minus [`UNSCANNED_FILES`], sorted so a failing assertion's offender list has a stable,
/// diffable order across runs. (The removed JS CLI's `packages/cli/lib/*.js` was dropped from this set
/// when the npm distribution was removed 2026-07-20 — there is no non-Rust surface left to scan.)
///
/// `crates/host/src/**/*.rs` was DROPPED in the 2026-07-26 teardown of that crate, and its
/// contributions here landed in directories this set already walks: `explain` (its bulk, and the part
/// that actually names config vocabulary) went to `crates/facade/src`, the embedded contract-resource
/// descriptions to `crates/summary/src`, and its pass-through dispatch was deleted outright. Since the
/// root set became derived (2026-07-28) a deleted crate simply stops being a member, so this needs no
/// bookkeeping; what still does is the RELOCATED files, pinned by name in
/// [`every_declared_scan_root_actually_contributes_files_to_the_census`], a file-scope `#[test]` below
/// (this file has no `mod tests`); the destinations were additionally confirmed during that teardown by
/// planting a violation in each and watching CHECK A/B go red.
///
/// **One file did NOT land inside this set, and saying otherwise was wrong** (corrected 2026-07-27).
/// CHECK A/B do not subtract test sources, so `crates/host/src/tools/tests.rs` WAS part of the old
/// census; it moved to `crates/summary/tests/host_dispatch.rs`, and every root is a `src` directory,
/// so it now falls outside. No live violation hides there — its only `--flag`-shaped tokens
/// are `--config` and `--version`, both vouched by `config-surface.json` — but the coverage is genuinely
/// gone, not relocated. Widening the census to `crates/*/tests` is deliberately NOT done here: it is a
/// scope decision (test sources spell vocabulary on purpose, so admitting them changes what CHECK A/B
/// mean for every root at once, not just this one) and needs its own decision rather than riding along
/// with a correction to a comment.
fn reference_validation_scanned_files() -> Vec<PathBuf> {
    let excluded = unscanned_files();
    let mut out = Vec::new();
    for rel in scan_roots() {
        out.extend(collect_root(&rel));
    }
    out.retain(|path| !fs::canonicalize(path).is_ok_and(|c| excluded.contains(&c)));
    out.sort();
    out
}

/// Contract #11, CHECK A — every `--flag`-shaped token on a non-comment line of every scanned file must
/// name a real CLI flag or a real external tool's flag (`config-surface.json`'s `cliFlags` ∪
/// `externalToolFlags`). This is the exact machine check that would have caught the shipped `--since=all`/
/// `--repo=` defects (see `reference_unit_tests.rs`'s `flag_reference_unit_tests` for those pinned as
/// unit tests).
///
/// **What this proves**: every `--flag`-shaped token reachable on a code line of a scanned source file
/// names a flag `config-surface.json` vouches for.
/// **What this CANNOT prove** (same "pragmatic proxy, not a semantic engine" caveat as this file's other
/// grep-based contracts): a flag built dynamically (`format!("--{name}")`) is invisible to this text scan;
/// a flag inside a STRING that is itself embedded in a doc comment example (as opposed to a real `//`/`/*`
/// prose line) is not distinguished from a real message — this is a textual proxy over source text, not an
/// AST-aware "is this reachable from a `Finding::message`" check.
#[test]
fn every_flag_reference_in_shipped_source_names_a_real_cli_or_external_tool_flag() {
    let vocab = load_config_surface();
    let mut offenders = Vec::new();
    for path in reference_validation_scanned_files() {
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        for flag in unknown_flag_references(&extract_flag_references(&text), &vocab) {
            offenders.push(format!("{}: `{flag}`", path.display()));
        }
    }
    assert!(
        offenders.is_empty(),
        "shipped source names a --flag that is not a real CLI flag or a real external tool flag (not in \
         config-surface.json's cliFlags/externalToolFlags — the exact defect class `--since=all`/`--repo=` \
         shipped as): {offenders:#?}"
    );
}

/// Contract #11, CHECK B — every backtick-quoted, config-key-shaped token sitting within 120 bytes of the
/// word "config" on a non-comment line of every scanned file must name a real config path/key
/// (`config-surface.json`'s `configPaths` ∪ `configKeys` ∪ `embedderFields` ∪ `allowlistedTokens`). This is
/// the exact machine check that would have caught the shipped `scanners.vocabulary.commitTypePatterns`
/// defect (see `reference_unit_tests.rs`'s `config_context_unit_tests` for that pinned as a unit test).
///
/// **Allowlist entries** (each earned, not padding — see `config-surface.json`'s own `_docs.allowlistedTokens`
/// for the summary, and this list for the exact source line each was found at):
/// - `zzop.config.jsonc` — the CLI's own config filename; not currently backtick-quoted anywhere in the
///   scanned tree (it appears as plain prose, e.g. `crates/metrics/src/diagnostics.rs`), allowlisted
///   preemptively so a future backtick-quoted mention does not spuriously fail.
/// - `Authorization` — `rules/native/rules-cross-layer/src/cross_layer/external_secret_in_url.rs`'s
///   `external-secret-in-url` message recommends moving a secret to an `` `Authorization` `` HTTP header;
///   that backtick sits ~50 bytes before the SAME message's own "Disable via config `rules: {...}`" clause,
///   putting it inside the 120-byte window purely by co-location, not because it names a config knob.
/// - `IoConsume` — `rules/native/rules-cross-layer/src/cross_layer/sdk_import_no_visible_consume.rs`'s
///   message names the `` `IoConsume` `` Rust fact type a Mode B adapter would project calls into; same
///   "shares a sentence with the disable hint" co-location, not a config reference.
/// - `crossLayer.unresolvedConsumes` — `rules/native/rules-cross-layer/src/cross_layer/unconsumed_endpoint.rs`'s
///   message points a reader at the `` `crossLayer.unresolvedConsumes` `` OUTPUT field (part of the JSON
///   `analyzeTrees()` returns, not an input config path) for corroborating evidence; same co-location
///   pattern.
/// - `require` — `rules/native/rules-graph/src/unreachable.rs`'s `unreachable` message's "Disable via
///   config `rules: {...}` ... if this island is reached by a mechanism this graph doesn't see (e.g.
///   dynamic `` `require` ``, a plugin loader)" aside names Node's `require()` as an example of an
///   invisible-to-the-graph reachability mechanism, not a config knob; same co-location pattern.
///
/// **What this proves**: every backtick-quoted, identifier/dotted-path-shaped token within 120 bytes of
/// "config" on a code line of a scanned source file names a real config path/key, embedder field, or
/// allowlisted non-config token.
/// **What this CANNOT prove**: a config-key reference with no backticks and no adjacent "config" text is
/// invisible to this scan (prose references are explicitly out of scope — see the module doc); a
/// dynamically-built message (`format!("`{key}`")`) is invisible the same way CHECK A's dynamic-flag gap
/// is.
#[test]
fn every_config_context_backtick_token_in_shipped_source_names_a_real_config_path_or_key() {
    let vocab = load_config_surface();
    let mut offenders = Vec::new();
    for path in reference_validation_scanned_files() {
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let tokens = extract_config_context_tokens(&text);
        for tok in unknown_config_context_tokens(&tokens, &vocab) {
            offenders.push(format!("{}: `{tok}`", path.display()));
        }
    }
    assert!(
        offenders.is_empty(),
        "shipped source has a backtick-quoted, config-key-shaped token near the word \"config\" that names \
         no real config path/key (not in config-surface.json's configPaths/configKeys/embedderFields/ \
         allowlistedTokens — the exact defect class `scanners.vocabulary.commitTypePatterns` shipped as): \
         {offenders:#?}"
    );
}

/// NON-EMPTINESS GUARD for the two checks above — the thing that makes their green mean something.
///
/// `collect_rs_files` degrades to "no files" on a missing directory by design, so a scan root that is
/// renamed, moved or deleted does not fail either check: it silently contributes zero files and both
/// tests keep passing over a narrower tree. That is not a hypothetical. It happened on 2026-07-26: the
/// `crates/host` teardown left this file's `mcp_src_files()` pointing at a deleted `../host/src`, and the
/// whole suite stayed green while one declared surface had dropped out of the census entirely. A guard
/// that passes while scanning nothing is the failure mode this repo has now measured three times.
///
/// So every scan root is asserted to contribute at least one file, and the two surfaces the teardown
/// RELOCATED are additionally asserted by name — those are the files whose coverage was silently lost,
/// and naming them means a future move has to come here and say so instead of quietly narrowing the
/// scan. Deliberately structural (does the census see the file?) rather than behavioral (does the file
/// pass?): the checks above already own the verdict.
///
/// The root list is read from [`scan_roots`], the same accessor the census builder reads — see its doc
/// for why this guard must not keep a list of its own. The two ways a root can contribute nothing (the
/// directory is GONE vs. the directory is there and holds no `.rs` file) are reported separately, since
/// they need different fixes and an empty `Vec` alone cannot tell them apart: a single message asserting
/// "renamed, moved or deleted" was diagnosing a cause it had not measured.
#[test]
fn every_declared_scan_root_actually_contributes_files_to_the_census() {
    let roots = scan_roots();
    assert!(
        !roots.is_empty(),
        "the derived scan-root set is EMPTY — CHECK A/B would then walk no file at all and pass \
         vacuously. The root Cargo.toml's `members` array is what this set is derived from; see \
         `workspace_members`."
    );
    for rel in &roots {
        let rel = rel.as_str();
        assert!(
            ws_dir(rel).is_dir(),
            "scan root {rel} DOES NOT EXIST — a workspace member with no `src` directory, so the walk \
             degraded silently to zero files instead of failing. Either the member is gone (drop it \
             from the root Cargo.toml's `members`) or it keeps its sources somewhere else (declare it \
             in `UNSCANNED_MEMBERS` with the reason) — do not leave a dead scan that keeps CHECK A/B \
             green over a narrower tree."
        );
        assert!(
            !collect_root(rel).is_empty(),
            "scan root {rel} EXISTS but contributed ZERO .rs files — nothing matches {}. Its sources \
             moved elsewhere. Either fix the member's layout or declare it in `UNSCANNED_MEMBERS` \
             with the reason — do not leave a dead scan that keeps CHECK A/B green over a narrower \
             tree.",
            root_glob(rel)
        );
    }

    // The census as the checks actually consume it must contain the relocated surfaces themselves,
    // not merely their directories: `explain` (host -> facade) and the embedded contract-resource
    // descriptions (host -> summary).
    //
    // Compared CANONICALIZED, not as path strings: every path here is built by joining `..` onto
    // `CARGO_MANIFEST_DIR`, so a census entry literally reads `crates/engine/../facade/src/explain.rs`
    // and a `ends_with("crates/facade/src/explain.rs")` test fails on a file that is present (measured —
    // this assertion's first version did exactly that). `cli_only_lane_sources` in the sibling
    // `surface_parity.rs` canonicalizes for the same reason.
    let census: BTreeSet<PathBuf> = reference_validation_scanned_files()
        .iter()
        .filter_map(|p| fs::canonicalize(p).ok())
        .collect();
    for relocated in [
        "crates/facade/src/explain.rs",
        "crates/summary/src/contracts.rs",
    ] {
        let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let expected = fs::canonicalize(workspace_root.join(relocated)).unwrap_or_else(|e| {
            panic!("{relocated} does not exist ({e}) — it was moved or deleted; re-point this pin")
        });
        assert!(
            census.contains(&expected),
            "{relocated} is not in the scanned census — it carries caller-facing text that names \
             config vocabulary, and it moved here in the 2026-07-26 `crates/host` teardown. If it \
             moved again, re-point the owning scan root rather than letting it fall out."
        );
    }
}

/// Every directory under the workspace root that holds a `Cargo.toml` (excluding the workspace root
/// itself), as workspace-relative slash-separated paths — the SECOND, independent derivation of "what
/// packages does this repo have", read off the filesystem instead of the manifest.
///
/// Four directory names are skipped, all for one reason — they hold code this repo did not write and
/// does not build: `target` and `node_modules` (generated/vendored dependency manifests), `corpus`
/// (the dogfood corpus's cloned OSS checkouts, git-ignored — `.gitignore`'s `/corpus/oss/`), and
/// dot-directories. Nothing else is filtered, so a package added anywhere else in the tree shows up
/// here whether or not anyone remembered the manifest.
fn packages_on_disk() -> BTreeSet<String> {
    fn walk(dir: &Path, rel: &str, out: &mut BTreeSet<String>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if name.starts_with('.')
                || name == "target"
                || name == "node_modules"
                || name == "corpus"
            {
                continue;
            }
            let child = if rel.is_empty() {
                name.to_string()
            } else {
                format!("{rel}/{name}")
            };
            if path.join("Cargo.toml").is_file() {
                out.insert(child.clone());
            }
            walk(&path, &child, out);
        }
    }
    let mut out = BTreeSet::new();
    walk(&ws_dir("."), "", &mut out);
    out
}

/// The guard that makes the DERIVATION honest — the leg [`every_declared_scan_root_actually_contributes_files_to_the_census`]
/// structurally cannot cover.
///
/// That test asks "does every root I derived contribute files?", which is satisfied just as well by a
/// derivation that produced too FEW roots — exactly the failure the hand-written array used to ship
/// (`crates/git` and `crates/cache` were never in it, and a planted `--since=all` in `crates/git`
/// stayed green). Deriving from the manifest does not by itself fix that: a textual `members = [`
/// extraction that silently drops entries narrows the census the same way, and every remaining root
/// still contributes files.
///
/// So the member list is reconciled against a second, independent derivation — the packages actually
/// on disk ([`packages_on_disk`]) — and the two must agree exactly. A package present on disk but
/// absent from `members` is either a crate nobody builds or a parse that lost it; a member with no
/// directory is a stale manifest. This is not a hypothetical check written for symmetry: it fired on
/// its first run, on the real tree, against the very parse it was written to back up (see
/// [`workspace_members`]' comment note).
///
/// [`UNSCANNED_FILES`] is checked here too, and against the census BEFORE subtraction: an exclusion
/// that no longer names a scanned file excludes nothing today and would silently start excluding
/// something else if the path were ever reused.
#[test]
fn the_derived_scan_set_covers_every_package_in_the_workspace() {
    let declared: BTreeSet<String> = workspace_members().into_iter().collect();
    let on_disk = packages_on_disk();
    assert!(
        !declared.is_empty(),
        "parsed ZERO workspace members out of the root Cargo.toml — the `members = [` extraction in \
         `workspace_members` broke, and every derived scan root went with it"
    );
    assert_eq!(
        declared,
        on_disk,
        "the workspace member list and the packages on disk disagree, so the derived scan set covers \
         less (or more) than this repo actually builds.\nmembers with no directory on disk: \
         {:?}\npackages on disk that no `members` entry names — their shipped messages are scanned by \
         NOTHING: {:?}",
        declared.difference(&on_disk).collect::<Vec<_>>(),
        on_disk.difference(&declared).collect::<Vec<_>>(),
    );

    let unsubtracted: BTreeSet<PathBuf> = scan_roots()
        .iter()
        .flat_map(|rel| collect_root(rel))
        .filter_map(|p| fs::canonicalize(p).ok())
        .collect();
    for (rel, reason) in UNSCANNED_FILES {
        let canonical = fs::canonicalize(ws_dir(rel)).unwrap_or_else(|e| {
            panic!("`UNSCANNED_FILES` names {rel}, which does not resolve ({e})")
        });
        assert!(
            unsubtracted.contains(&canonical),
            "`UNSCANNED_FILES` excludes {rel} ({reason}), but no scan root contributes that file — \
             the exclusion subtracts NOTHING today, and would silently subtract a real file the \
             moment that path is reused. Drop the entry."
        );
    }
}
