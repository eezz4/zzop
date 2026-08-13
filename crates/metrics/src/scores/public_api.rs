//! Public API ??ratio of cross-module imports that bypass another module's index barrel (deep-path imports). A
//! module's index/barrel file is its public contract; reaching past it into internal files couples callers to
//! implementation details that are free to change.

use super::config::ScoresConfig;
use super::detail_cap::{cap_and_count_dropped, MAX_EDGE_ROWS_LISTED};
use super::shared::{is_external, module_root, round};
use super::types::{DeepImport, PublicApiScore};
use zzop_core::DepGraph;

/// `"src/".len()`.
const SRC_PREFIX_LEN: usize = 4;

pub fn compute_public_api(
    dep: &DepGraph,
    cfg: &ScoresConfig,
    is_scored: &dyn Fn(&str) -> bool,
) -> PublicApiScore {
    let mut deep: Vec<DeepImport> = Vec::new();
    let mut total: u32 = 0;

    // Deterministic traversal: HashMap iteration order is unspecified, so sorting by the importer path
    // gives a stable, reproducible order.
    // Subject here is the IMPORTER (`from`), never the target ??see `ScoresInput::is_scored`.
    let mut froms: Vec<&String> = dep.keys().filter(|from| is_scored(from)).collect();
    froms.sort();

    for from in froms {
        let fm = module_root(cfg, from);
        for to in &dep[from] {
            if is_external(to) {
                continue;
            }
            let tm = match module_root(cfg, to) {
                Some(m) => m,
                None => continue,
            };
            if fm.as_deref() == Some(tm.as_str()) {
                continue;
            }
            total += 1;
            if !is_root_import(to, &tm) {
                deep.push(DeepImport {
                    from: from.clone(),
                    to: to.clone(),
                    to_module: tm,
                });
            }
        }
    }

    deep.sort_by(|a, b| a.to_module.cmp(&b.to_module));

    let score = if total == 0 {
        100.0
    } else {
        (100.0 - (deep.len() as f64 / total as f64) * 100.0).max(0.0)
    };

    let deep_imports_truncated = cap_and_count_dropped(&mut deep, MAX_EDGE_ROWS_LISTED);

    PublicApiScore {
        score: round(score),
        total_cross_module_imports: total,
        deep_imports: deep,
        deep_imports_truncated,
    }
}

/// True when `to` resolves to a file sitting DIRECTLY under `module` ??its barrel/index or any other
/// top-level file, all of which count as the module's public surface ??false when it reaches into a
/// subdirectory (a deep import).
///
/// # The `is_index_barrel` call that used to be here was provably dead (removed 2026-08-12)
///
/// The test read `is_index_barrel(after_root) || !after_root.contains('/')`, which looks like "a barrel,
/// or a root-level file". It was one test: `is_index_barrel`'s regex is fully anchored
/// (`^index\.(?:tsx?|jsx?|mjs|cjs)$`), so every string it accepts is free of `/` and therefore already
/// satisfies the right-hand side. No input could reach the second operand having failed the first, and
/// nothing this crate can be handed would change that. Deleting it changes no verdict ??the deletion was
/// landed on that basis, not on a measurement, because there is nothing to measure.
///
/// What the dead clause LOOKED like it did is the real gap, and it is deliberately still open: a NESTED
/// barrel (`pkg/sub/index.ts`) is judged a deep import here. The sibling metric disagrees ??/// `shared::is_upward_import` (hierarchy) applies the identical `is_index_barrel` to the BASENAME
/// (`shared.rs`, `to.rsplit('/')`), so the same file is a barrel there and a deep import here. Widening
/// this side to match was considered and deferred (2026-08-12 user decision): it MOVES scores, and the
/// evidence to move them is missing rather than negative ??across seven JS/TS corpus repos the metric
/// produced 10 cross-module edges in total, because `module_root` collapses to the first path segment
/// and a single-`src/` layout therefore has almost no cross-module imports to judge. A widening decided
/// on a sample that small would be a guess wearing a measurement's clothes. Note also that the
/// disagreement is not JS-specific: `pkg/sub/__init__.py` and `pkg/sub/mod.rs` are read as deep imports
/// by BOTH metrics, since the regex knows only the JS/TS spellings.
fn is_root_import(to: &str, module: &str) -> bool {
    let stripped = strip_leading_dotdot(to);
    let prefix = format!("{}/", module);
    let mut after_root = stripped.strip_prefix(prefix.as_str()).unwrap_or("");
    if let Some(rest) = after_root.strip_prefix("src/") {
        debug_assert_eq!("src/".len(), SRC_PREFIX_LEN);
        after_root = rest;
    }
    !after_root.contains('/')
}

fn strip_leading_dotdot(p: &str) -> &str {
    let mut s = p;
    while let Some(rest) = s.strip_prefix("../") {
        s = rest;
    }
    s
}

// Two test modules, in their own files since 2026-08-13 (per-file line cap). The split is by
// SUBJECT, which is the separation this file already maintained inline: `tests` asks what the metric
// computes, `legend_tests` asks whether the sentence shipped beside the number still describes that
// computation. Same shape as the crate's `health.rs`/`health/tests.rs` pairing.
#[cfg(test)]
mod legend_tests;
#[cfg(test)]
mod tests;
