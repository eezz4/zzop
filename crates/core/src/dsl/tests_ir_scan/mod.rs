//! IR-query matcher tests: symbol-scan (per-file declaration queries) and io-scan (whole-tree IO-fact
//! queries, evaluated via `eval_pack_io_scan` + `IoScanTreeContext` since the 2026 projection redesign).
//! Split into `symbol_scan_tests` / `io_scan_tests` / `gates_tests` purely to stay under the repo's
//! per-file line cap (`scripts/check-max-file-lines.sh`) — one logical test suite across the three files.

mod gates_tests;
mod io_scan_tests;
mod symbol_scan_tests;
