//! Config LOADING — the entry points that turn a path into a [`crate::LoadedRequest`], plus the one
//! proactive disclosure that rides their warnings channel. Split out of `lib.rs` on 2026-08-08 when
//! the envelope-lane entry pushed that file over the line ratchet; the cut is along the seam the crate
//! root already had — `lib.rs` keeps the crate doc, the two public types and the module wiring, and
//! everything that READS A FILE lives here.

use std::path::{Path, PathBuf};

use crate::{
    io_error_label, jsonc, mapper, workspaces, ConfigError, LoadedRequest, Method,
    DEFAULT_CONFIG_FILENAME,
};

/// Loads the config for a single analyzed root: reads `<root>/zzop.config.jsonc`, or REFUSES.
///
/// Errors only on caller-fixable config problems (`ConfigError`), never on overlay/pack content issues
/// (those become warnings). When the result is a single tree over a root whose workspace manifest names
/// 2+ packages, the leading disclosure warning from [`disclose_single_tree_over_workspace`] is prepended —
/// the request stays byte-identical either way, the run just stops being silent about the join it could
/// not attempt.
///
/// # There is no zero-config run (2026-07-27, reversing the founding default)
///
/// This used to synthesize the request an empty `{}` config would produce. That became unsafe the day
/// convention vocabulary stopped falling back to built-ins (`zzop_engine::vocabulary`): no config means
/// no declaration, and no declaration means those judgments are not made — the run would analyze less
/// while reporting itself complete. Total refusal rather than "warn and continue", because a warning
/// beside a shorter findings list is exactly the disclosure readers skip. [`missing_config_error`] owns
/// the message; both hosts hit this same function, so the entry axis has no parity exception.
pub fn load_for_root(root: &Path) -> Result<LoadedRequest, ConfigError> {
    let candidate = root.join(DEFAULT_CONFIG_FILENAME);
    if candidate.is_file() {
        return load_config_file(&candidate);
    }
    Err(missing_config_error(&candidate))
}

/// Whether a load may emit [`disclose_single_tree_over_workspace`].
///
/// That disclosure is ADVICE ABOUT A TREE WALK — it counts workspace packages and tells the reader to
/// add `"trees": "auto"` so those packages become separate trees, "at the cost of the dependency graph,
/// which is built per tree". Every clause of it presumes a run that walks a tree.
#[derive(Clone, Copy, PartialEq, Eq)]
enum TreeAdvice {
    Disclose,
    /// The caller will use ONLY the convention vocabulary and walks nothing — see
    /// [`load_for_root_vocabulary_only`].
    Omit,
}

/// [`load_for_root`] for a caller that takes ONLY the convention vocabulary and walks no tree: the
/// envelope lane (Mode A), where the envelope file REPLACES native parsing.
///
/// Identical to `load_for_root` except that the single-tree-over-workspace disclosure is omitted,
/// because on this lane all three of its claims are false: the run analyzed **zero** trees rather than
/// a single one, `"trees"` is not among the keys an envelope run reads, and there is no per-tree
/// dependency graph to trade away. Observed 2026-08-08 in a real reply, where it landed **directly
/// after** the envelope lane's own disclosure saying the vocabulary block "is the ONLY thing an
/// envelope run takes from an adjacent config: every other key there configures a tree analysis …
/// none of which an envelope run does" — two adjacent sentences, one telling the reader to set a key
/// the other had just said is ignored.
///
/// Every OTHER warning still comes through. An unknown-config-key report is about the file, not the
/// lane, and an envelope caller needs it exactly as much as a tree caller does; the split here is by
/// what the warning ASSERTS ABOUT THE RUN, not by convenience.
pub fn load_for_root_vocabulary_only(root: &Path) -> Result<LoadedRequest, ConfigError> {
    let candidate = root.join(DEFAULT_CONFIG_FILENAME);
    if candidate.is_file() {
        return load_config_file_with(&candidate, TreeAdvice::Omit);
    }
    Err(missing_config_error(&candidate))
}

/// The stable head of [`missing_config_error`]'s message — the one token a HOST may match on to
/// recognize this refusal and append its own prescription (see below). Public so the display layers
/// never hand-copy the literal: `ConfigError` is a plain `String` by the time `summary` flattens it,
/// so without a shared constant each host would grep for a phrase it spelled itself, and the two
/// copies would drift apart exactly the way every hand-copied literal in this repo has.
pub const MISSING_CONFIG_MARKER: &str = "No config file at ";

/// The one refusal message for "this analysis has no config", shared by every lane and both hosts.
///
/// Names the ARTIFACT and never a command. A message saying `zzop init` is unactionable for an MCP client
/// (it has no shell), and one saying `resources/read` is unactionable in a terminal; the `config-template`
/// contract document is the single thing both hosts can serve, and it is the same bytes `init` writes.
/// Machine-pinned by `crates/engine/tests/rule_contracts/host_vocabulary.rs` (contracts 15 & 16), which
/// scans this crate for either host's vocabulary.
///
/// THE SHARED STRING IS ONLY HALF THE ANSWER, by design (2026-08-09 user ruling, reversing the earlier
/// reading of this doc): each HOST appends its own spelling of the way out at its own display layer —
/// the MCP server's `orientation` text prescribes reading `zzop://contract/config-template`, and the
/// `zzop` CLI appends a `Run \`zzop init\`` line when it recognizes [`MISSING_CONFIG_MARKER`]. Until
/// that ruling the CLI was the one host that appended nothing, so terminal users got the artifact name
/// and no runnable next step while MCP clients got a full prescription — host neutrality of THIS string
/// was never meant to mean no host may be helpful in its own voice.
pub fn missing_config_error(candidate: &Path) -> ConfigError {
    ConfigError(format!(
        "{MISSING_CONFIG_MARKER}{}.\nzzop analyzes what a config DECLARES: the convention vocabulary a \
         project picks — what it calls its auth guards, which banners mark its generated files, how it \
         names its data-access receivers — has no built-in default, so a run without a config would \
         judge less while reporting itself complete. Start from the `config-template` contract document \
         (every zzop surface can serve it) and save it as {}.",
        candidate.display(),
        candidate.display()
    ))
}

/// Loads an explicit config file (or a directory containing `zzop.config.jsonc`) and maps it with
/// the config file's directory as the resolution base. This is the multi-tree/cross-repo entry: the
/// config's `trees` (including `trees: "auto"`) defines the join.
///
/// Unlike the JS CLI's `loadConfig` (which only ever reads the exact path it is given), `path` here
/// may also name a DIRECTORY containing `zzop.config.jsonc` — a Rust-host convenience with no JS
/// counterpart (the JS CLI never receives a bare directory; `bin/zzop.js` always resolves `--config`
/// or the default filename before calling `loadConfig`).
///
/// A config that never declares `trees` still resolves to ONE tree, so it gets the same leading
/// [`disclose_single_tree_over_workspace`] disclosure the config-less path gets (worded for the
/// "add it to your existing config" remedy) — having a config file is not evidence that the tree
/// set was chosen.
pub fn load_config_file(path: &Path) -> Result<LoadedRequest, ConfigError> {
    load_config_file_with(path, TreeAdvice::Disclose)
}

/// The shared body of [`load_config_file`] and [`load_for_root_vocabulary_only`]. One body, so the two
/// entries can only ever differ in the single thing `advice` names — a second copy is how the
/// config-less and config-bearing paths once drifted into teaching two different remedies for the
/// same missing file (see [`missing_config_error`]).
fn load_config_file_with(path: &Path, advice: TreeAdvice) -> Result<LoadedRequest, ConfigError> {
    let candidate = if path.is_dir() {
        path.join(DEFAULT_CONFIG_FILENAME)
    } else {
        path.to_path_buf()
    };

    // Same condition, same message as the discovery path — one owner, so the two entries can never
    // teach a user two different remedies for the identical missing file.
    if !candidate.is_file() {
        return Err(missing_config_error(&candidate));
    }

    let raw = std::fs::read_to_string(&candidate).map_err(|err| {
        ConfigError(format!(
            "Could not read config at {}: {}",
            candidate.display(),
            io_error_label(&err)
        ))
    })?;

    let stripped = jsonc::strip_json_comments(&raw);
    let parsed: serde_json::Value = serde_json::from_str(&stripped)
        .map_err(|err| ConfigError(format!("Invalid JSONC in {}: {err}", candidate.display())))?;

    if !parsed.is_object() {
        return Err(ConfigError(format!(
            "Config in {} must be a JSON object.",
            candidate.display()
        )));
    }

    let base_dir = candidate
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    let (config, mut warnings) = maybe_expand_auto_trees(parsed, &base_dir)?;
    let mapped = mapper::config_to_request(&config, &base_dir)?;
    warnings.extend(mapped.warnings);
    if advice == TreeAdvice::Disclose {
        disclose_single_tree_over_workspace(
            mapped.method,
            &base_dir,
            &candidate,
            &config,
            &mut warnings,
        );
    }
    Ok(LoadedRequest {
        method: mapped.method,
        request: mapped.request,
        warnings,
        config_path: Some(candidate),
    })
}

/// Prepends `workspaces::single_tree_workspace_warning` (when it fires) to `warnings` — the ONE
/// place a load discloses "this run is a single tree over a monorepo, so the cross-layer join never
/// ran". Only [`load_config_file`] reaches it now: since 2026-07-27 there is no config-less load, so
/// the config path and the config itself are always in hand rather than optional.
///
/// Gated on `Method::Analyze`, which is exactly the structural fact worth warning about: by this
/// enum's own contract it means one tree resulted AND `trees` was never declared. Keying on that
/// (rather than on "no config file was found") is what makes a config carrying only
/// `{"rules": {...}}` — a real monorepo trap, since no `trees` means no expansion and therefore no
/// expansion report to speak in its place — disclose exactly like the config-less case, while an
/// author who DID declare `trees` (any explicit array, or `"auto"`) is never second-guessed: those
/// take the `AnalyzeTrees` branch and never reach here. See that function's doc for the rest of the
/// silence contract.
///
/// Position: `insert(0, ..)`, never `push` — this is a leading disclosure, and pinning the index
/// keeps the warning list byte- AND order-deterministic no matter how many mapping notes precede it.
/// Purely additive: the request, method and tree set are untouched.
fn disclose_single_tree_over_workspace(
    method: Method,
    base_dir: &Path,
    config_path: &Path,
    config: &serde_json::Value,
    warnings: &mut Vec<String>,
) {
    if method != Method::Analyze {
        return;
    }
    if let Some(warning) = workspaces::single_tree_workspace_warning(base_dir, config_path, config)
    {
        warnings.insert(0, warning);
    }
}

/// Thin `workspaces::expand_auto_trees` gate: that function is a documented no-op for any config
/// whose `trees` is not EXACTLY the string `"auto"` (see its own module doc), so this short-circuits
/// the common case without calling into it at all. Purely an implementation courtesy — the observable
/// result is identical to an unconditional call once `workspaces.rs` is implemented; it just means
/// `zzop-config`'s own build/tests for every other config shape don't take a hard dependency on that
/// (separately owned) module being finished first.
fn maybe_expand_auto_trees(
    config: serde_json::Value,
    base_dir: &Path,
) -> Result<(serde_json::Value, Vec<String>), ConfigError> {
    if config.get("trees").and_then(serde_json::Value::as_str) == Some("auto") {
        workspaces::expand_auto_trees(config, base_dir)
    } else {
        Ok((config, Vec::new()))
    }
}
