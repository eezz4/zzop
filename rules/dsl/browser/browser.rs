//! Exercises `rules/dsl/browser/browser.json`'s browser-footgun rules end-to-end via
//! `zzop_engine::analyze_tree`, routing through real engine dispatch rather than calling the line-scan
//! matcher directly.
//!
//! Bare `confirm()`/`alert()`/`prompt()` trip `no-system-dialogs`; `document.write`/`writeln` trip
//! `no-document-write`. `Matcher::LineScan` reports at most one finding per line per rule, so fixtures
//! spread one call per line to exercise each independently. `window.confirm`/`globalThis.alert` are
//! flagged with the receiver kept in the snippet; a call on an unrelated receiver, or a clean file,
//! produces no findings.
//!
//! `// browser-ok` (on the finding's line or the 3 lines above) suppresses `no-system-dialogs`;
//! `// document-write-ok` does the same for `no-document-write` — distinct markers, since a shared one
//! would let suppressing one rule silently suppress the other (the `rule_contracts` meta-test checks this).
//!
//! `postmessage-wildcard` flags `postMessage(..., '*')`; `// postmessage-target-ok` suppresses it.
//! `unsafe-html-sink` flags a non-literal `.innerHTML`/`.outerHTML` assignment (`innerhtml-assign`), a
//! backtick template assignment that interpolates (`innerhtml-template`), `insertAdjacentHTML(...)` whose
//! html argument isn't a plain literal (`insert-adjacent`), or JSX `dangerouslySetInnerHTML={{ __html: ... }}`
//! whose value isn't a plain literal (`dangerously-set`); `// unsafe-html-ok` suppresses it. Both rules are
//! source-free (unlike `security/taint-flow`, which needs a request-derived source in the same function and
//! only looks at `.ts`/`.tsx`) and cover `.js`/`.jsx` too.
//!
//! `javascript-url` flags a literal `javascript:`-scheme URL in a link/src position: a JSX/HTML attribute
//! literal (`jsx-href-literal`), a `.href`/`.src` property assignment (`href-assign`), or a `setAttribute`
//! call (`setattr-js`); `// javascript-url-ok` suppresses it. Scope-limited to the literal form — a dynamic
//! `href={x}` is not attempted.
//!
//! `location-assign-dynamic` flags a non-literal assignment to the client navigation sink (`location`/
//! `window.location`/`location.href` = ..., or `location.assign(...)`/`location.replace(...)`);
//! `// location-assign-ok` suppresses it. Receiver-aware like `no-document-write` (an arbitrary object's
//! own `.location` field, e.g. `user.location = x`, is never matched — only the bare global/`window.`
//! form is); a literal string/`/`-prefixed path stays silent; a `const location = ...` declaration
//! (the React Router `useLocation()` shape) is excluded via `exclude_pattern`.
//!
//! `jquery-html-sink` flags a jQuery HTML-insertion method (`.html`/`.append`/`.prepend`/`.after`/
//! `.before`/`.wrapAll`) called with a non-literal argument, gated on the file mentioning `jquery` or a
//! `$(` call; `// jquery-html-ok` suppresses it.
//!
//! `vue-v-html` flags Vue's `v-html` directive (`.vue` files, plus `.ts`/`.tsx`/`.js`/`.jsx`);
//! `// vue-v-html-ok` suppresses it.
//!
//! `unsanitized-markdown-html` (method-scan) flags a markdown-render call (`marked(`/`markdownit(`/
//! `md.render(`/`remark(`/`showdown(`) co-occurring with an HTML sink (`innerHTML`/`outerHTML`/
//! `dangerouslySetInnerHTML`/`v-html`) in the same function span, with no `DOMPurify`/`sanitize`/
//! `sanitizeHtml`/`xss` token anywhere in that span; `// markdown-html-ok` suppresses it. `.vue` is in its
//! file pattern for forward-compatibility only — this engine has no symbol/span parser for `.vue` today
//! (see `docs/rules/dsl-reference.md`'s method-scan doc), so a `.vue` SFC's `<script>`+`<template>`
//! pairing never actually co-fires; only same-file `.ts`/`.tsx`/`.js`/`.jsx` co-occurrence does.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zzop_core::RulePackDef;
use zzop_engine::{analyze_tree, AnalyzeOutput, DispatchConfig, EngineConfig, DEFAULT_SIZE_CAP};

mod dialogs;
mod javascript_url;
mod jquery_html;
mod location_assign;
mod location_assign_guards;
mod markdown_html;
mod postmessage;
mod unsafe_html;
mod vue_v_html;

struct TempDir(PathBuf);

impl TempDir {
    fn new(prefix: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("{prefix}-{}-{nanos}-{n}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn write(&self, rel: &str, content: &str) {
        let full = self.0.join(rel);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(full, content).unwrap();
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Loads the real `browser.json` pack from the repo, one directory down from this test file.
fn browser_pack() -> RulePackDef {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("dsl/browser/browser.json");
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
    serde_json::from_str(&text).expect("parse browser.json")
}

fn config() -> EngineConfig {
    EngineConfig {
        source_id: "browser-fixture".to_string(),
        dispatch: DispatchConfig::default(),
        size_cap: DEFAULT_SIZE_CAP,
        rule_config: Default::default(),
        packs: vec![browser_pack()],
        ..EngineConfig::default()
    }
}

fn scan(dir: &TempDir) -> AnalyzeOutput {
    analyze_tree(dir.path(), &config())
}
