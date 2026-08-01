//! S13 — the inherited-`[Route]` self-report for C# controllers.
//!
//! `attribute_controller` reads a class's OWN `[Route]` attribute only. A controller that inherits its
//! prefix from a project base class in ANOTHER file therefore gets an EMPTY prefix, and its methods are
//! keyed at their own path alone — `GET /users` where the deployment serves `GET /api/v1/users`.
//!
//! That is the gateway-rewrite shape again (S12): not a silence but a WRONG KEY, so the join reports an
//! unprovided consume for a route that is actually served. The module already blocks rather than guesses
//! when a class's own `[Route]` argument is non-literal; this case cannot be blocked the same way,
//! because blocking needs to know the base carries a route and that is exactly what is not readable.
//!
//! ## The discriminator, and why the naive versions are worthless in BOTH directions
//! EVERY ASP.NET controller derives from `ControllerBase` or `Controller`. Warning on "has a base type"
//! would fire on all of them. What matters is a base that is NOT one of the framework's own — a project
//! base class, the only kind that can carry a prefix zzop did not read.
//!
//! The first cut of this module got the other direction wrong and the measurement is why this one is
//! shaped as it is. It scanned every `class X : Y` line in any `[Http`/`[Route`-bearing file, so a
//! nested DTO (`class Request : BaseRequest`) inside an ordinary `ControllerBase` controller produced a
//! warning whose every word was false — while on `corpus/oss/be-aspnet`, **8 of 8 controllers** use the
//! C# 12 primary-constructor spelling (`class UsersController(IMediator m) : ApiBaseController`), which
//! that cut could not parse at all. Noisy where nothing was wrong, silent on the entire real corpus. So
//! this version mirrors the extractor's own controller gate ([`attribute_controller`]'s
//! `[ApiController]`/`[Controller]`-or-name-suffix test) and parses the spellings that corpus actually
//! uses.
//!
//! Lexical, not AST: this is a warning, never a finding, and its job is to say "go look". The parser
//! that would resolve the base properly is the FEATURE this line reports as absent. The declared limits
//! of the lexical scan are listed on [`scan::class_declarations`] — a limit that is not written down is a
//! silence, and silence is this family's fatal direction.
//!
//! [`attribute_controller`]: parser-csharp's `adapters::provides::attribute_controller`

use std::collections::BTreeSet;
use std::path::Path;

mod scan;

use scan::{class_declarations, declares_own_route, strip_comments};

/// Base types that carry no project-specific route prefix. Deriving from one of these is the ordinary
/// ASP.NET shape and says nothing about an inherited `[Route]`.
const FRAMEWORK_BASES: &[&str] = &[
    "ControllerBase",
    "Controller",
    "ApiController",
    "ODataController",
];

/// Controllers named in the message, at most this many.
const MAX_EXAMPLES: usize = 3;

/// How far below a class's name the base-list `:` may sit. C# allows the base list on its own line;
/// two is enough for every wrapped spelling seen, and a bound keeps a runaway scan off a minified file.
const BASE_LIST_LOOKAHEAD: usize = 2;

/// One warning when this tree has C# controller classes that derive from a PROJECT base class and
/// declare no `[Route]` of their own — the shape whose prefix zzop cannot read. `None` otherwise.
pub fn csharp_base_route_warning(root: &Path, cs_files: &[String]) -> Option<String> {
    let mut suspects: Vec<String> = Vec::new();
    // A `partial` controller is one class written as several declarations; counting each half would
    // report two hazards where the deployment has one.
    let mut counted: BTreeSet<String> = BTreeSet::new();
    for rel in cs_files {
        let Ok(text) = std::fs::read_to_string(root.join(rel)) else {
            continue;
        };
        // A file with no controller-ish surface cannot contribute a mis-keyed route.
        if !text.contains("[Http") && !text.contains("[Route") {
            continue;
        }
        let lines = strip_comments(&text);
        for class in class_declarations(&lines) {
            if !class.is_controller || FRAMEWORK_BASES.contains(&class.base.as_str()) {
                continue;
            }
            // The class declares its own prefix -> nothing is inherited that matters.
            if declares_own_route(&lines, class.line) {
                continue;
            }
            if !counted.insert(class.name.clone()) {
                continue;
            }
            if suspects.len() < MAX_EXAMPLES {
                suspects.push(format!("{} : {} ({rel})", class.name, class.base));
            }
        }
    }
    let total = counted.len();
    if total == 0 {
        return None;
    }
    Some(format!(
        "Inherited C# route prefix(es) not read: {total} controller class(es) derive from a PROJECT \
         base class and declare no `[Route]` of their own, e.g. {}. zzop reads a class's OWN `[Route]` \
         only, so if the base carries the prefix these routes are keyed WITHOUT it — `GET /users` where \
         the app serves `GET /api/v1/users`. That is a wrong key rather than a missing one, so the \
         cross-layer join will report an unprovided consume for a route that is actually served. \
         Declare the effective prefix with `trees[].topology.mountedAt` in your config (or \
         `trees[].routes` if only some controllers carry it — a shared base class is not necessarily \
         one directory, which is what `.mounts` keys on), or move the `[Route]` onto the controller \
         itself.",
        suspects.join(", ")
    ))
}

#[cfg(test)]
#[path = "csharp_base_route_tests.rs"]
mod tests;
