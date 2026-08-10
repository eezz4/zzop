// Strips OUR OWN release version out of the manifest bytes that `FP_ENGINE` hashes.
//
// ## Why this exists
// `FP_ENGINE` suffixes EVERY arm of `cache::parser_fingerprint`, and the workspace root `Cargo.toml`
// plus `Cargo.lock` are in its hashed inputs. Both carry our release version, and `Cargo.lock` carries
// it once per workspace member. So before this filter, bumping the version for a release moved every
// arm and cost every user a 100% cache miss — the exact coupling `CACHE_SCHEMA_VERSION` shed on
// 2026-08-05 when its `{release}+{hash}` form was cut down to the hash alone. Cutting it there and
// leaving it here would have left the decision half-landed: the schema version stopped wiping the
// cache, and the per-entry keys kept missing it anyway.
//
// ## What is deliberately NOT stripped
// A DEPENDENCY's version. That is the whole reason these two files joined the hash: `swc_core = "71.0.5"`
// is a caret range, so `cargo update` can move the resolved frontend with no source byte changing, and
// the resolved version lands in `Cargo.lock`. Strip too much and the hole reopens silently.
//
// ## How ours is told apart from theirs — structurally, never by name
// In the lock file a `[[package]]` block with no `source` key is a workspace member; registry and git
// packages always carry one. Matching on the crate NAME instead would be a hand-maintained list with
// the drift this build script exists to eliminate.
//
// ## Compiled twice, on purpose
// Once as a module of this crate (so these unit tests run at all, and so `dir_fingerprint` sees the file
// — an edit here moves the very fingerprint it shapes), and once via `include!` from `build.rs`, which
// cannot depend on the crate it builds. That is also why the header above uses `//` and not `//!`:
// an inner doc comment is illegal where `include!` splices this in.

/// `version = "..."` as a key in its own right — not `version.workspace = true`, and not the `version`
/// INSIDE an inline table like `serde = { version = "1" }` (that one never starts its line).
#[allow(dead_code)]
fn is_own_version_key(trimmed: &str) -> bool {
    match trimmed.strip_prefix("version") {
        Some(rest) => rest.trim_start().starts_with('='),
        None => false,
    }
}

/// One `[[package]]` block of a `Cargo.lock`, minus its `version` line if the block is a workspace
/// member. `in_package` is false for the file preamble and for trailing non-package sections, whose
/// `version` (the LOCK FORMAT version) must survive — it is not ours and it does mean something.
#[allow(dead_code)]
fn push_lock_block(out: &mut String, block: &[&str], in_package: bool) {
    let is_workspace_member = in_package && !block.iter().any(|line| line.starts_with("source = "));
    for line in block {
        if is_workspace_member && is_own_version_key(line.trim_start()) {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
}

/// The bytes of `rel` as `FP_ENGINE` should hash them. Anything other than the two workspace manifests
/// passes through untouched, as does a file that is not valid UTF-8 — hashing it whole is the safe
/// direction (over-invalidation), and guessing at a shape we cannot parse is not.
#[allow(dead_code)]
fn without_own_release_version(rel: &str, bytes: &[u8]) -> Vec<u8> {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return bytes.to_vec();
    };
    match rel {
        "Cargo.toml" => {
            let mut out = String::with_capacity(text.len());
            let mut in_workspace_package = false;
            for line in text.lines() {
                let trimmed = line.trim_start();
                if trimmed.starts_with('[') {
                    // `[workspace.dependencies]` holds THEIR versions and must be left alone; only the
                    // bare `version` of `[workspace.package]` is the release SSOT.
                    in_workspace_package = trimmed.starts_with("[workspace.package]");
                }
                if in_workspace_package && is_own_version_key(trimmed) {
                    continue;
                }
                out.push_str(line);
                out.push('\n');
            }
            out.into_bytes()
        }
        "Cargo.lock" => {
            let mut out = String::with_capacity(text.len());
            let mut block: Vec<&str> = Vec::new();
            let mut in_package = false;
            for line in text.lines() {
                // Only a section header sits at column 0 starting with `[`; a dependency array's
                // continuation lines are indented and its closing bracket starts with `]`.
                if line.starts_with('[') {
                    push_lock_block(&mut out, &block, in_package);
                    block.clear();
                    in_package = line.starts_with("[[package]]");
                }
                block.push(line);
            }
            push_lock_block(&mut out, &block, in_package);
            out.into_bytes()
        }
        _ => bytes.to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(bytes: Vec<u8>) -> String {
        String::from_utf8(bytes).unwrap()
    }

    #[test]
    fn workspace_package_version_goes_but_dependency_versions_stay() {
        let manifest = "[workspace]\nmembers = [\"a\"]\n\n\
                        [workspace.package]\nversion = \"0.29.1\"\nedition = \"2021\"\n\n\
                        [workspace.dependencies]\nserde = { version = \"1\" }\nswc_core = \"71.0.5\"\n";
        let out = s(without_own_release_version(
            "Cargo.toml",
            manifest.as_bytes(),
        ));
        assert!(!out.contains("version = \"0.29.1\""), "ours must go: {out}");
        assert!(
            out.contains("serde = { version = \"1\" }"),
            "theirs must stay"
        );
        assert!(out.contains("swc_core = \"71.0.5\""), "theirs must stay");
        assert!(
            out.contains("edition = \"2021\""),
            "unrelated keys must stay"
        );
    }

    #[test]
    fn lock_strips_only_blocks_without_a_source() {
        let lock = "# auto-generated\nversion = 4\n\n\
                    [[package]]\nname = \"zzop-engine\"\nversion = \"0.29.1\"\n\n\
                    [[package]]\nname = \"swc_core\"\nversion = \"71.0.5\"\nsource = \"registry+https://x\"\n";
        let out = s(without_own_release_version("Cargo.lock", lock.as_bytes()));
        assert!(
            !out.contains("version = \"0.29.1\""),
            "workspace member's version must go"
        );
        assert!(
            out.contains("version = \"71.0.5\""),
            "a registry dep's version is the SIGNAL"
        );
        assert!(
            out.contains("version = 4"),
            "the lock FORMAT version is not ours to strip"
        );
        assert!(
            out.contains("name = \"zzop-engine\""),
            "only the version line is dropped"
        );
    }

    /// The property that actually matters, stated as the release it describes: bumping the workspace
    /// version — and every member version the lock repeats it into — must not move the hashed bytes.
    /// Two versions apart, and a DOWNGRADE, are the same case; equality is what the cache compares.
    #[test]
    fn a_release_bump_does_not_move_the_hashed_bytes() {
        let lock_at = |v: &str| {
            format!(
                "version = 4\n\n\
                 [[package]]\nname = \"zzop-core\"\nversion = \"{v}\"\ndependencies = [\n \"serde\",\n]\n\n\
                 [[package]]\nname = \"zzop-engine\"\nversion = \"{v}\"\n\n\
                 [[package]]\nname = \"serde\"\nversion = \"1.0.200\"\nsource = \"registry+https://x\"\n"
            )
        };
        let root_at = |v: &str| format!("[workspace.package]\nversion = \"{v}\"\n");

        for other in ["0.29.1", "0.31.0", "0.28.0"] {
            assert_eq!(
                without_own_release_version("Cargo.lock", lock_at("0.29.1").as_bytes()),
                without_own_release_version("Cargo.lock", lock_at(other).as_bytes()),
                "lock bytes moved between 0.29.1 and {other}"
            );
            assert_eq!(
                without_own_release_version("Cargo.toml", root_at("0.29.1").as_bytes()),
                without_own_release_version("Cargo.toml", root_at(other).as_bytes()),
                "root manifest bytes moved between 0.29.1 and {other}"
            );
        }
    }

    /// The other half, and the one that goes wrong quietly: a DEPENDENCY bump must still move the bytes.
    /// Without this, a strip that was slightly too greedy would look like a success.
    #[test]
    fn a_dependency_bump_still_moves_the_hashed_bytes() {
        let lock_at = |v: &str| {
            format!("[[package]]\nname = \"swc_core\"\nversion = \"{v}\"\nsource = \"registry+https://x\"\n")
        };
        assert_ne!(
            without_own_release_version("Cargo.lock", lock_at("71.0.5").as_bytes()),
            without_own_release_version("Cargo.lock", lock_at("71.9.0").as_bytes()),
            "a resolved dependency version is exactly what this input exists to carry"
        );
    }

    #[test]
    fn other_files_and_non_utf8_pass_through_whole() {
        let bytes = b"version = \"0.29.1\"\n";
        assert_eq!(
            without_own_release_version("crates/engine/Cargo.toml", bytes),
            bytes.to_vec(),
            "member manifests inherit the version and are not this filter's subject"
        );
        let invalid = vec![0xff, 0xfe, 0x00];
        assert_eq!(
            without_own_release_version("Cargo.lock", &invalid),
            invalid,
            "unparseable input is hashed whole — over-invalidation is the safe direction"
        );
    }
}
