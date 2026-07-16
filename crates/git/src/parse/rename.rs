//! Numstat rename-path spellings — `parse_path` (and its slash-collapse helper), split out of
//! `parse.rs` unchanged; the parent module's pinned rename semantics (see its module doc) live here.

/// Handles the `git --numstat -M` rename path spellings: a top-level `"old => new"` rename, and the
/// common-prefix-optimized `"{old => new}"` fragment inside an otherwise-shared path (e.g.
/// `"src/{old.ts => new.ts}"` or `"{a => b}/file.ts"`). Only ONE `{...}` fragment per path is
/// handled — git-numstat never emits more than one.
pub(super) fn parse_path(raw: &str) -> (String, Option<String>) {
    if let Some(brace_start) = raw.rfind('{') {
        if let Some(arrow_rel) = raw[brace_start..].find(" => ") {
            let arrow_at = brace_start + arrow_rel;
            if let Some(close_rel) = raw[arrow_at..].find('}') {
                let close_at = arrow_at + close_rel;
                let pre = &raw[..brace_start];
                let old_mid = &raw[brace_start + 1..arrow_at];
                let new_mid = &raw[arrow_at + 4..close_at];
                let post = &raw[close_at + 1..];
                let old_p = collapse_slashes(&format!("{pre}{old_mid}{post}"));
                let new_p = collapse_slashes(&format!("{pre}{new_mid}{post}"));
                return (new_p, Some(old_p));
            }
        }
    }
    if let Some(idx) = raw.find(" => ") {
        let old_p = raw[..idx].to_string();
        let new_p = raw[idx + 4..].to_string();
        return (new_p, Some(old_p));
    }
    (raw.to_string(), None)
}

/// Mirrors `.replace(/\/{2,}/g, "/")` — the brace-splice can produce a doubled slash at the junction
/// (e.g. `pre` ending in `/` and `post` starting with `/` when the old/new segment is empty).
fn collapse_slashes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_slash = false;
    for c in s.chars() {
        if c == '/' {
            if prev_slash {
                continue;
            }
            prev_slash = true;
        } else {
            prev_slash = false;
        }
        out.push(c);
    }
    out
}
