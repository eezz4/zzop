## Summary

<!-- What does this change do, and why? -->

## Checklist

- [ ] Gates pass locally, with the same flags CI uses — a bare `cargo test` / `cargo clippy`
      passes on strictly less than CI does, and that drift is what turned a release tag red:
      `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
      `cargo test --workspace`
- [ ] Guards pass locally: `git config core.hooksPath .githooks` once per clone, then every
      `git commit` runs the full guard set. To run them without committing:
      `bash .githooks/pre-commit`. (Do **not** use `bash scripts/*.sh` — the shell passes the
      remaining paths as arguments to the first script, so exactly one guard runs and it exits 0.)
- [ ] English-only (docs, comments, commit message)
- [ ] docs/site updated if user-facing behavior changed
- [ ] No version bumps — versions come from release tags
