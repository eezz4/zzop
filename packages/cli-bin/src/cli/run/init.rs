//! `zzop init` — the one subcommand that WRITES into a tree rather than reading it. Split out of
//! `run.rs` because the pair would exceed the 300-line file budget.

use super::super::print_or_exit;

/// `zzop init [<dir>] [--force]`: writes the embedded `config-template` document — the ONE canon behind all
/// three surfaces (this, `zzop contract config-template`, MCP `resources/read`) — to the config filename in
/// `<dir>` (default: the current directory); argv parsing plus one file write, no template text. An existing file is never
/// overwritten without `--force`: a RUNTIME refusal (exit 1) like `diff`'s, where a bad argument is exit 2.
pub fn run_init(args: &[String]) -> ! {
    let mut force = false;
    let mut dir: Option<&str> = None;
    for a in &args[2..] {
        if a == "--force" {
            force = true;
            continue;
        }
        // A positional is the TREE to set up (`zzop init ./fe`), and it must already exist. That
        // requirement is not politeness — it is what keeps `zzop init adapter` an error. The old JS
        // scaffolder took a subcommand there and was retired in 2026-07-26; a positional that created
        // whatever directory it was handed would silently bring that shape back, spelled as a typo.
        if dir.is_some() || !std::path::Path::new(a).is_dir() {
            eprintln!(
                "usage: zzop init [<dir>] [--force] (unexpected argument {a:?}) -- <dir> must be an \
                 existing directory; this subcommand writes a config INTO a tree and never creates one"
            );
            std::process::exit(2);
        }
        dir = Some(a);
    }
    let base = std::path::Path::new(dir.unwrap_or("."));
    let target = base.join(zzop_summary::contracts::CONFIG_TEMPLATE_FILENAME);
    let doc = zzop_summary::contracts::find(zzop_summary::contracts::CONFIG_TEMPLATE_NAME)
        .expect("the config-template document is embedded in this binary");
    if !force && target.exists() {
        eprintln!(
            "zzop: {} exists — pass --force to overwrite it",
            target.display()
        );
        std::process::exit(1);
    }
    let written = std::fs::write(&target, doc.content)
        .map(|()| format!("wrote {}", target.display()))
        .map_err(|e| format!("failed to write {}: {e}", target.display()));
    let note = match written {
        Ok(ref msg) => match ensure_cache_dir_ignored(base) {
            Ok(Some(line)) => format!("{msg}\n{line}"),
            Ok(None) => msg.clone(),
            // A .gitignore this command could not touch is worth SAYING, not worth failing on: the
            // config landed and the tree is usable. Silence here would leave the user with the exact
            // trap this step exists to close, and no hint that it was attempted.
            Err(e) => {
                format!("{msg}\nnote: could not update .gitignore ({e}) — add `**/.zzop/` yourself")
            }
        },
        Err(_) => String::new(),
    };
    print_or_exit(written.map(|_| note));
}

/// Append the anchored cache-dir ignore to `<base>/.gitignore`, unless that exact pattern is already
/// there. Returns the line it wrote, or `None` when nothing was needed.
///
/// **Anchored (`**/.zzop/`), never a `zzop*` glob.** The derived cache dir and the hand-authored
/// `zzop/` rule-pack dir differ by one leading dot, so a glob takes the user's own rule packs out of
/// version control with no error from git and none from zzop — `docs/getting-started.md` spends a
/// section on that trap. A command whose entire job is setting a tree up should not leave it armed.
fn ensure_cache_dir_ignored(base: &std::path::Path) -> std::io::Result<Option<String>> {
    const PATTERN: &str = "**/.zzop/";
    let path = base.join(".gitignore");
    let existing = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e),
    };
    if existing.lines().any(|l| l.trim() == PATTERN) {
        return Ok(None);
    }
    let mut next = existing;
    if !next.is_empty() && !next.ends_with('\n') {
        next.push('\n');
    }
    // The comment deliberately does NOT spell the dangerous glob. A .gitignore is a file of patterns;
    // a pattern-shaped string sitting in a comment is one careless uncomment away from hiding the
    // user's hand-authored `zzop/` rule packs, which is the exact accident this line exists to prevent.
    next.push_str(
        "# zzop's derived cache. Anchored on purpose: a wildcard here would also hide the\n\
         # hand-authored zzop/ directory (rule packs, adapters), silently and with no error.\n",
    );
    next.push_str(PATTERN);
    next.push('\n');
    std::fs::write(&path, next)?;
    Ok(Some(format!("added `{PATTERN}` to {}", path.display())))
}
