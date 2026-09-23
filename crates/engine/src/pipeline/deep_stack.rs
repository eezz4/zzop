//! The stack the per-file pass runs on, and why it is not the default one.
//!
//! ## The defect this exists for (2026-09-07, external review round 12, review ledger V99)
//!
//! Every parser in this workspace is a recursive-descent parser, so nesting depth in the SOURCE becomes
//! stack depth in the process. Past a threshold the thread overflows, and a stack overflow is not a
//! panic: it aborts. The `catch_unwind` wrappers in [`super::parsers`] cannot see it — that machinery
//! was built for the 2026-07-29 panic incident, and this is a different failure mode with the same blast
//! radius. Measured at HEAD before this module existed, one file per tree, `zzop analyze <dir>`:
//!
//! | input, valid in its own language        | depth  | exit |
//! |-----------------------------------------|--------|------|
//! | Java   `return (((…1…)))`                | 5,000  | 127  |
//! | Go / Rust / C# nested parens             | 20,000 | 127  |
//! | Rust   `{{{…}}}`                         | 20,000 | 127  |
//! | TS     `type T = Array<Array<…>>`        | 2,000  | 127  |
//! | TS     `let q = 1 + 1 + …`               | 5,000  | 127  |
//! | JS     `"x" + "x" + …` (30,000 terms)    | 180 KB | 127  |
//!
//! Exit 127, empty stdout, and stderr carrying only `thread '<unknown>' has overflowed its stack`. No
//! JSON, no warning naming the file, no partial answer for the other 1,950 files in the tree. Through
//! `zzop-mcp` it is worse: the `tools/call` never returns and the server disappears from the client.
//!
//! ## Why this is a thread-stack fix and not another gate
//!
//! [`super::fresh::recursion`] already holds a refusal gate, and the gate is the reason this went unseen.
//! It is scoped to TypeScript because its module doc asserted that "only the TypeScript frontend dies:
//! the other seven return `degraded: 1` at depth 100,000" — false for at least Java, Go, Rust and C#,
//! as the table above shows. And it counts only `(`, `[` and `{`, so the two shapes that actually kill
//! the TypeScript parser (`<`-nesting and long binary-operator chains) walk straight through the gate
//! built for TypeScript. Widening a bracket census per language would be one more approximation of the
//! same shape, wrong in a new direction each time a parser changes.
//!
//! Stack headroom answers the whole class at once, for every language, with no per-language census to
//! keep true. It does not make the failure impossible — a deep enough input still overflows — it moves
//! the floor from "a 10 KB generated file" to far past anything a repository holds. The gate stays, for
//! the genuinely pathological, and now guards a floor it can state honestly.
//!
//! ## Why the memory cost is not the R1 cost
//!
//! `stack_size` RESERVES address space; pages commit on first touch. On 64-bit hosts the reservation is
//! free in the terms `architecture.md`'s R1 ledger cares about — that ledger bounds RSS, and untouched
//! stack pages are not resident. A tree of shallow files touches the same few pages it always did.

use std::sync::Mutex;

use rayon::ThreadPoolBuilder;

/// Stack reserved per parsing thread.
///
/// 64 MiB, chosen against the measurements above rather than picked: the shallowest observed death was
/// TypeScript `<`-nesting at depth 2,000 on the default rayon stack, and the deepest hand-written
/// nesting in a 2,120-file census of this corpus was 6. The reserve clears every shape in that table by
/// a wide margin, and the margin is the point — a floor that only just clears today's measurement is a
/// floor that fails on the next parser change.
pub(crate) const PARSE_STACK_BYTES: usize = 64 * 1024 * 1024;

/// Runs `f` on threads that all have [`PARSE_STACK_BYTES`] of stack.
///
/// Both halves matter. The rayon pool covers its worker threads; the dedicated thread covers the
/// CALLER, because `ThreadPool::install` blocks the calling thread and lets it participate in
/// work-stealing — so without it, one file per pass could still be parsed on whatever stack the caller
/// happened to have. Missing that is how a fix like this ships and still aborts one run in eight.
///
/// Scoped threads rather than `'static` ones because the pass borrows the walk, the config and the
/// cache handle; nothing here is cloned to satisfy a thread bound.
///
/// If the pool cannot be built or the thread cannot be spawned, `f` runs on the current stack. That is
/// the pre-2026-09-07 behaviour — worse, but not a new failure. Refusing to analyze because a thread
/// could not be spawned would turn a rare deep file into a total outage for every shallow one.
pub(crate) fn with_parse_stack<T, F>(f: F) -> T
where
    F: FnOnce() -> T + Send,
    T: Send,
{
    let slot: Mutex<Option<F>> = Mutex::new(Some(f));
    let out: Mutex<Option<T>> = Mutex::new(None);

    // Takes `f` and runs it on whatever stack the caller of this closure has, inside a big-stack pool.
    let run = || {
        let g = slot
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
            .expect("with_parse_stack runs its closure exactly once");
        let v = match ThreadPoolBuilder::new()
            .stack_size(PARSE_STACK_BYTES)
            .build()
        {
            Ok(pool) => pool.install(g),
            Err(_) => g(),
        };
        *out.lock().unwrap_or_else(|e| e.into_inner()) = Some(v);
    };

    std::thread::scope(|scope| {
        match std::thread::Builder::new()
            .stack_size(PARSE_STACK_BYTES)
            .spawn_scoped(scope, run)
        {
            Ok(handle) => {
                // Propagating preserves the panic-to-`catch_unwind` path the parser wrappers rely on;
                // swallowing it would turn a real panic into a silently empty pass.
                if let Err(payload) = handle.join() {
                    std::panic::resume_unwind(payload);
                }
            }
            Err(_) => {
                let g = slot
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .take()
                    .expect("with_parse_stack runs its closure exactly once");
                *out.lock().unwrap_or_else(|e| e.into_inner()) = Some(g());
            }
        }
    });

    out.into_inner()
        .unwrap_or_else(|e| e.into_inner())
        .expect("with_parse_stack always produces a value")
}
