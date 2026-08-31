//! §33/§37 LANDING for the two rules whose remedy takes a SHELL out of the loop —
//! `command-and-interpolation` (Rust) and `command-interpolated-string` (Python/Go).
//!
//! WHAT THE READER'S CORRECT EDIT COSTS (`1.architecture/rules/rule-quality.md` §27 leg 3). Both
//! messages end in the same two instructions: pass every argument as its own element, and stop routing
//! through a shell (`sh -c`, `shell=True`, `os.system`). Where the shell was only an injection vector
//! that edit is free. Where the shell was DOING SOMETHING — a pipe, a redirect, `&&`/`;` sequencing, a
//! glob, `$VAR`, `~` — the argv form does not do it, and the failure is a wrong result rather than an
//! error: the first program runs against the remaining tokens as literal filenames. Neither message
//! said so, while both spent a paragraph explaining that the shell is what makes the TypeScript
//! sibling `critical` — so both already knew the shell was load-bearing and told the reader to remove
//! it anyway.
//!
//! ONE CONSTANT FOR THE TWO, byte-identical. The property belongs to the shell, not to what either
//! rule detects: `Command::new("sh").arg("-c")`, `subprocess.run(..., shell=True)`, `os.system` and
//! `exec.Command("sh", "-c", ...)` all hand one string to the same parser, so the cost is the same
//! sentence in Rust, Python and Go — and the delivered fixture of each carrier is a shell-routed call,
//! which is what makes the sharing a measurement rather than a guess.
//!
//! WHY NOT `ARGV_REWRITE_LANDING`, the closest sibling (§37's most-dangerous-reuse test). That
//! constant names `exec`/`execSync` in its own bytes — Node APIs that appear in no Rust, Python or Go
//! program — so splicing it here would ship a sentence about a function the reader does not call.
//! Its mechanism half is the same; its subject is not, and §37 settled that a landing whose noun is
//! wrong for the rule gets a new constant rather than a reused one.
//!
//! WHY THE JAVA SIBLING (`cmd-injection`) IS NOT A THIRD CARRIER, recorded here so the next author
//! does not "complete the family" by pasting this in. `Runtime.exec(String)` hands its string to NO
//! shell — that rule's own message says so at length, and it is why the rule is `warning` rather than
//! `critical`. Its cost is the WHITESPACE TOKENIZATION `exec(String)` performs and `exec(String[])`
//! does not, which is a different mechanism with a different failure, so it carries
//! `EXEC_TOKENIZATION_LANDING` in `java_moved_rules.rs` instead. Shipping this constant there would
//! tell a Java reader that their pipes break, in a call that never had a pipe.
//!
//! NOT A DISQUALIFIER. A reader whose command uses a pipe still has a real finding: the interpolation
//! is still reaching a shell, which is exactly the defect. What changes is that the mechanical rewrite
//! is not available to them.
//!
//! POSITION, not presence. The invalidation probe for both tests is to move this constant to the tail
//! of the message: every token stays present and spelled exactly once, and each pin goes red on ORDER.

use crate::{assert_landing_precedes_imperative, hits, scan, TempDir};

const SHELL_ROUTING_LANDING: &str = "WHERE A SHELL WAS DOING THE WORK, THE ARGV FORM DOES NOT DO IT, AND WHAT BREAKS FAILS QUIETLY: a `|` pipe, a `>` redirect, `&&`/`;` sequencing, a `*` glob, `$VAR` expansion and a `~` home reference are parsed by the shell, so the moment those parts become inert argv elements the command stops meaning what it meant — it does not raise, it runs the first program against the rest as literal filenames. The whitespace split goes with it: one interpolated value that used to expand into several tokens now arrives as ONE argument, and the called program rejects it or reads it as a filename. Read the command string before you rewrite it — with no shell metacharacter in it the split is mechanical, and with one the pipeline has to move into the calling program: wire the pipe through the child stdio, write the child output yourself instead of redirecting, expand the glob with a directory read. Turning the shell back on to make the argv form work again restores the exact injection this finding is about.";

/// Rust. The fixture is `Command::new("sh").arg("-c").arg(script)` — the shell-routed spelling this
/// rule's own message singles out as the injection-bearing one, and therefore the exact population
/// whose reader is told to stop routing through `sh -c`.
#[test]
fn command_and_interpolation_landing_precedes_the_imperative() {
    let dir = TempDir::new("zzop-sec-rust");
    dir.write(
        "src/jobs.rs",
        "use std::process::Command;\npub fn run(period: &str) -> std::io::Result<std::process::Output> {\n    let script = format!(\"/usr/local/bin/report --period {}\", period);\n    Command::new(\"sh\").arg(\"-c\").arg(script).output()\n}\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "command-and-interpolation");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_landing_precedes_imperative(
        "command-and-interpolation",
        &h[0].message,
        SHELL_ROUTING_LANDING,
        "Pass every argument as its own `.arg(value)`",
    );
}

/// Python. `subprocess.run(f"...", shell=True)` — the same shape one language over, and the one this
/// rule's message calls command injection outright because the whole string reaches a shell.
#[test]
fn command_interpolated_string_landing_precedes_the_imperative() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "app/tools.py",
        "import subprocess\n\ndef run(name):\n    subprocess.run(f\"ls {name}\", shell=True)\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "command-interpolated-string");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    assert_landing_precedes_imperative(
        "command-interpolated-string",
        &h[0].message,
        SHELL_ROUTING_LANDING,
        "Pass every argument as its own list element",
    );
}

/// The facts the landing carries, and the one exit it must keep naming as a TRAP. Turning the shell
/// back on is the move a reader makes when the argv rewrite breaks their pipeline, it makes both these
/// rules go quiet, and it restores the substrate the finding is about — an exit that read as an option
/// here would be an exit back into the defect.
#[test]
fn the_shell_routing_landing_keeps_its_worked_costs_and_its_trap() {
    let dir = TempDir::new("zzop-be-sec");
    dir.write(
        "app/tools.py",
        "import subprocess\n\ndef run(name):\n    subprocess.run(f\"ls {name}\", shell=True)\n",
    );
    let out = scan(&dir);
    let h = hits(&out, "command-interpolated-string");
    assert_eq!(h.len(), 1, "{:?}", out.findings);
    for needle in [
        "it does not raise, it runs the first program against the rest as literal filenames",
        "now arrives as ONE argument",
        "Read the command string before you rewrite it",
        "expand the glob with a directory read",
    ] {
        assert!(
            h[0].message.contains(needle),
            "security/command-interpolated-string: the landing lost {needle:?}: {}",
            h[0].message
        );
    }
    let trap = h[0]
        .message
        .find("Turning the shell back on")
        .expect("the shell-restore trap left the message");
    assert!(
        h[0].message[trap..].contains("restores the exact injection this finding is about"),
        "security/command-interpolated-string: re-enabling the shell is named but no longer as a \
         trap: {}",
        h[0].message
    );
}
