//! The CALL-SITE channel ([`CallSite`]) — "this file used API family X at line N, spelled like this".
//!
//! One fact for a class of evidence that is otherwise re-derived per language, per rule, as a regex over
//! raw text: a console write, an environment read, a process spawn, a hash call. A text regex gets three
//! things wrong that a projected fact does not — it fires inside string literals and comments, it needs a
//! new copy per language, and it cannot be crossed with a STRUCTURAL gate ("was this call inside a loop"),
//! because raw text has no spans.
//!
//! # What is deliberately NOT carried, and why that is the design
//!
//! `callee` is the raw spelling and it is the ONLY name channel. There is no `level`, no `stream`, no
//! `severity` field, and folding one in would be a FALSE FOLD rather than a convenience: JavaScript's
//! `console.error` carries its level as a member-name TAG, Python's `print(file=sys.stderr)` carries its
//! stream as an ARGUMENT, and only Rust's `log::error!` names a real severity. Those three are not the same
//! fact, so a single `level: Option<String>` field could only be filled by inventing an equivalence the
//! source never stated. The channel therefore carries spelling + position + family, and every semantic
//! judgment ("this is an error log", "this is dangerous") stays in the rule, where `CallScan::callee_pattern`
//! can read the spelling the author actually wrote.
//!
//! # Never-guess
//!
//! A call whose callee cannot be resolved statically emits NO site at all — not a site with an empty or
//! approximated `callee`. Same rule `IoConsume::key` follows for an unresolvable target: the absence is
//! honest, an approximation would be a claim the source does not make. A consequence worth stating: this
//! channel under-reports on purpose, so a rule reading it must treat silence as "no evidence", never as
//! "no violation".

use serde::{Deserialize, Serialize};

/// The API family a call site belongs to. Open-ended (`String`, not an enum) for the same reason
/// [`crate::io::IoKind`] is: a Mode-B adapter or an external producer may introduce a family this build
/// never heard of, and an enum would make that a wire-breaking change. Casing convention is kebab-case,
/// matching io kinds and rule ids ([`CALL_KIND_CONSOLE_WRITE`] and friends are the spellings this build
/// ships).
///
/// Openness has the same failure mode io's does, so it takes the same antidote: a producer can emit
/// `"process-exec"` today, the sites will ride the cache and reach [`crate::dsl::RuleContext`], and they
/// will produce ZERO findings, because every rule filters on a kind literal. [`RULE_READ_CALL_KINDS`] is
/// the one place that says which kinds a rule actually reads, so a run can DISCLOSE that it carried facts
/// nothing consumes rather than looking clean.
pub type CallKind = String;

/// `"console-write"` — a write to the process's console/standard streams (`console.log`, `print`,
/// `System.out.println`, `fmt.Println`).
///
/// Deliberately NOT this family: a structured logger (`logging`, `slf4j`, `ILogger`, `log`/`tracing`).
/// Folding those in would be false — a logger call is configured output with levels and sinks, and a rule
/// that bans console writes in a backend is not banning logging. Boundary cases and per-language
/// exclusions are the PRODUCER's to disclose in its own module doc; this constant only fixes the spelling.
pub const CALL_KIND_CONSOLE_WRITE: &str = "console-write";

/// `"env-read"` — a read of a process environment variable (`process.env.X`, `os.environ[...]`,
/// `System.getenv`, `std::env::var`).
///
/// Deliberately NOT this family: Rust's `env!()`, which is resolved at COMPILE time and reads no process
/// environment at run time. Same boundary discipline as [`CALL_KIND_CONSOLE_WRITE`]'s.
pub const CALL_KIND_ENV_READ: &str = "env-read";

/// `"process-exec"` — a statically witnessed construction/launch of an OS process
/// (`child_process.exec`, `subprocess.run`, `Runtime.getRuntime().exec`, `new ProcessBuilder`,
/// `Process.Start`, `exec.Command`, `Command::new`).
///
/// The site says a process API was USED at this line — never that the command is tainted, shell-bound,
/// or dangerous; those judgments belong to the rules (W3 keeps their co-occurrence evidence LEXICAL on
/// purpose — the channel structures only the exec witness). Two boundary disciplines carried from the
/// siblings: a third-party wrapper (`execa`, `tokio::process`) is not the platform's API and is each
/// producer's disclosed exclusion, and a receiver reached through a VARIABLE (`rt.exec(cmd)`,
/// `pb.start()`) is not a resolvable spelling and emits nothing — the one deliberate widening is a
/// FIXED platform chain (`Runtime.getRuntime().exec`) or a CONSTRUCTOR (`new ProcessBuilder`), each
/// argued in its producer's module doc.
pub const CALL_KIND_PROCESS_EXEC: &str = "process-exec";

/// `"hash-call"` — a statically witnessed construction of a cryptographic digest
/// (`crypto.createHash("md5")`, `hashlib.md5()`, `MessageDigest.getInstance("MD5")`, `MD5.Create()`,
/// `md5.New()`).
///
/// The one family that carries [`CallSite::algorithm`], and the reason that field exists at all — see
/// its doc for the never-guess contract (`Some` only when the source SPELLS the algorithm at the site,
/// `None` otherwise, never an inference). The site says "a digest was constructed here, spelled thus";
/// whether the algorithm is WEAK is the consuming rule's judgment (`algorithm_pattern`), exactly as
/// severity judgments live in `callee_pattern` for the console family. Third-party hash wrappers
/// (commons-codec's `DigestUtils`, npm digest packages) are each producer's disclosed exclusion, same
/// boundary discipline as [`CALL_KIND_PROCESS_EXEC`]'s.
pub const CALL_KIND_HASH_CALL: &str = "hash-call";

/// The call kinds this build's RULES actually read — the kinds some shipped rule names in a
/// `CallScan::kind` (or compares against a literal).
///
/// The obligation is [`crate::RULE_READ_IO_KINDS`]'s, verbatim: listing a kind no rule reads makes any
/// disclosure built on this list LIE (it goes quiet about facts nothing acts on), and omitting one a rule
/// does read makes it cry wolf.
///
/// **What subtracts against it today is the contract test, not a run.** io's list feeds a live per-run
/// self-report (`zzop_engine::framework_silence::unread_io_kind`); the call side has no such report yet,
/// so this constant's present job is narrower and worth stating plainly rather than implying the wider
/// one: it is the machine-checked ANSWER to "which call kinds does this build act on", kept true so that
/// the report — when a run carrying an unread kind is something that can actually happen, i.e. when an
/// external producer can inject call sites — has a correct set to subtract from on its first day.
///
/// **Every entry was earned in the wave that landed its readers, and the list is machine-bound.** W1
/// landed the TypeScript and Python producers together with the three rules that read them —
/// `reliability/console-in-be` and `reliability/console-in-loop` name [`CALL_KIND_CONSOLE_WRITE`] in
/// their `CallScan::kind`, `reliability/env-outside-config` names [`CALL_KIND_ENV_READ`] — so neither
/// entry is the speculative claim this list's predecessor (an honest `&[]`) refused to make. W3 added
/// [`CALL_KIND_PROCESS_EXEC`] in the wave that pointed the three exec rules' STRUCTURAL gates at it —
/// `security/shell-exec-interpolation`'s `LineScan::line_call_kind`, `security/cmd-injection`'s and
/// `security/command-and-interpolation`'s `MethodScan::require_call_kind` — a second declarative
/// spelling surface the binding test reads alongside `CallScan::kind`. W4 added
/// [`CALL_KIND_HASH_CALL`] in the wave that pointed `security/weak-password-hash` and
/// `security/weak-crypto` (both now `CallScan`s over it) at the family.
///
/// The binding is `crates/engine/tests/rule_contracts/call_kind_readers.rs`, io's twin
/// (`io_kind_readers.rs`) turned on this axis. It differs from io's in ONE way, and the difference is a
/// property of where each vocabulary is spelled: an io kind is compared by Rust code, so io's test greps
/// `kind == "..."` out of non-test source, whereas a call kind is named DECLARATIVELY in a shipped JSON
/// pack, so the call-side test reads the loaded packs' declarative kind fields (`CallScan::kind`,
/// `MethodScan::require_call_kind`, `LineScan::line_call_kind`) directly — the same data the engine
/// itself loads, with no grep proxy in between. A future Rust reader of a call kind would be invisible
/// to that test, which is why this doc, not the test, is the place that says so.
pub const RULE_READ_CALL_KINDS: &[&str] = &[
    CALL_KIND_CONSOLE_WRITE,
    CALL_KIND_ENV_READ,
    CALL_KIND_PROCESS_EXEC,
    CALL_KIND_HASH_CALL,
];

/// One statically witnessed call site. Category ② in the structural-fact projection contract (a
/// DSL-facing per-file fact — see `zzop_cache::FileIrSlice`'s module doc for what that membership
/// obligates): it is projected per file, cached per file, and read directly by
/// [`crate::dsl::Matcher::CallScan`].
///
/// `#[serde(rename_all = "camelCase")]` is a no-op today (every field is one word) — applied for
/// consistency with the sibling fact types, so a later multi-word field cannot silently ship in snake_case.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallSite {
    /// The API family — see [`CallKind`] for why this is an open `String` and what stops that openness
    /// from becoming a silent extension point.
    pub kind: CallKind,
    /// 1-based source line of the call. A site whose line the producer cannot place is not emitted
    /// (never-guess); consumers may defensively skip a `0` rather than anchor a finding nowhere.
    pub line: u32,
    /// The callee EXACTLY AS WRITTEN at the site — `"console.error"`, `"System.err.println"`,
    /// `"os.environ.get"`. Not normalized, not lowercased, not stripped of its receiver: the spelling IS
    /// the evidence a rule matches on (see this module's doc for why no `level`/`stream` field exists to
    /// pre-digest it). A call whose callee cannot be resolved statically produces no site at all.
    pub callee: String,
    /// The algorithm the site SPELLS, when it spells one — `Some("md5")` for
    /// `crypto.createHash("md5")` (a string-literal argument) and `Some("MD5")` for `MD5.Create()` /
    /// `MessageDigest.getInstance("MD5")` / `md5.New()` (the type, function or package name IS the
    /// algorithm), each kept exactly as written. `None` for every other family, and — the load-bearing
    /// half — for a hash call whose algorithm the source does NOT spell at the site
    /// (`createHash(algoVar)`, `hashlib.new(name)`): never-guess, so a rule filtering on
    /// `algorithm_pattern` goes SILENT there rather than approximating, and that silence is the
    /// channel's declared recall direction.
    ///
    /// This is the ONE argument-derived fact the channel carries, and it is a designed exception rather
    /// than a crack in the no-argument-capture wall (the projection contract's wave table names it):
    /// it is admitted only because a consuming rule cannot exist without it — "weak hash" is a judgment
    /// ABOUT the algorithm, where console/env/exec rules judge the callee alone. `#[serde(default)]`
    /// keeps every pre-W4 cache entry and producer valid: an absent field IS `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub algorithm: Option<String>,
}
