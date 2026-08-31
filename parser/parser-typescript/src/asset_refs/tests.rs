//! `asset_refs` capture tests — every sink plus the never-guess boundaries. Split out of
//! `asset_refs.rs` for the 300-line cap when the sixth sink landed; the seam is the obvious one, since
//! the parent is a visitor with no state and these only ever call it through `parse_asset_refs`.

use super::parse_asset_refs;

fn refs(src: &str) -> Vec<String> {
    parse_asset_refs("x.ts", src)
}

#[test]
fn audio_worklet_add_module_is_captured() {
    assert_eq!(
        refs("await ctx.audioWorklet.addModule(\"/noise/worklet.js\");\n"),
        vec!["/noise/worklet.js".to_string()]
    );
}

#[test]
fn add_module_on_non_audioworklet_receiver_is_skipped() {
    // A same-named `.addModule` on an unrelated object is not an asset load.
    assert!(refs("registry.addModule(\"/x.js\");\n").is_empty());
}

#[test]
fn new_worker_and_shared_worker_first_arg() {
    assert_eq!(
        refs("const w = new Worker(\"/w/a.js\", { type: \"module\" });\n"),
        vec!["/w/a.js".to_string()]
    );
    assert_eq!(
        refs("new SharedWorker(\"./b.js\");\n"),
        vec!["./b.js".to_string()]
    );
}

#[test]
fn import_scripts_is_variadic() {
    assert_eq!(
        refs("importScripts(\"/a.js\", \"/b.js\");\n"),
        vec!["/a.js".to_string(), "/b.js".to_string()]
    );
}

#[test]
fn new_url_with_import_meta_url_is_captured() {
    assert_eq!(
        refs("const u = new URL(\"./worker.ts\", import.meta.url);\n"),
        vec!["./worker.ts".to_string()]
    );
}

#[test]
fn new_url_without_import_meta_url_is_skipped() {
    // A bare `new URL("https://…")` is a real URL, not an asset reference.
    assert!(refs("const u = new URL(\"https://api.example.com/v1\");\n").is_empty());
    assert!(refs("const u = new URL(\"/x.js\", someBase);\n").is_empty());
}

#[test]
fn nested_new_worker_new_url_is_captured() {
    // The canonical Vite/webpack worker idiom — the `new URL(...)` is the FIRST ARG of `new Worker`,
    // not a separate statement. `new Worker`'s own first-arg capture sees a non-literal and yields
    // nothing; the nested `new URL(_, import.meta.url)` must still be captured by the child visit.
    assert_eq!(
        refs("const w = new Worker(new URL(\"./worker.ts\", import.meta.url));\n"),
        vec!["./worker.ts".to_string()]
    );
    assert_eq!(
        refs(
            "const w = new Worker(new URL(\"./worker.ts\", import.meta.url), { type: \"module\" });\n"
        ),
        vec!["./worker.ts".to_string()]
    );
    assert_eq!(
        refs("new SharedWorker(new URL(\"../shared/w.ts\", import.meta.url));\n"),
        vec!["../shared/w.ts".to_string()]
    );
}

#[test]
fn nested_new_worker_with_computed_url_is_skipped() {
    // never-guess: a computed/interpolated URL argument yields NO reference at all.
    assert!(refs("new Worker(new URL(name, import.meta.url));\n").is_empty());
    assert!(refs("new Worker(new URL(`./${name}.ts`, import.meta.url));\n").is_empty());
}

#[test]
fn non_literal_arg_is_skipped() {
    assert!(refs("const p = getPath(); new Worker(p);\n").is_empty());
}

#[test]
fn no_substitution_template_is_captured_but_interpolated_is_not() {
    assert_eq!(
        refs("new Worker(`/w/a.js`);\n"),
        vec!["/w/a.js".to_string()]
    );
    assert!(refs("new Worker(`/w/${name}.js`);\n").is_empty());
}

#[test]
fn a_guarded_service_worker_registration_is_captured_and_rebased_to_served_root() {
    // koel's `resources/assets/js/app.ts:30` verbatim — and the guarded spelling matters twice
    // over: `navigator.serviceWorker?.register(...)` parses as an `OptCall`, which the plain
    // call visitor never sees, and it is the spelling real code uses because
    // `navigator.serviceWorker` is undefined outside a secure context.
    //
    // The value is rebased, not raw. This is the ONLY sink whose argument the browser resolves
    // against the PAGE rather than the module, so `'./sw.js'` means served-root `/sw.js`
    // (`public/sw.js`) and never `resources/assets/js/sw.js`. Handing the raw string to a
    // module-relative resolver would resolve to nothing, which is indistinguishable from having no
    // sink at all — the failure this rebase exists to prevent.
    assert_eq!(
        refs("navigator.serviceWorker?.register(\"./sw.js\");\n"),
        vec!["/sw.js".to_string()]
    );
    assert_eq!(
        refs("navigator.serviceWorker.register(\"./sw.js\");\n"),
        vec!["/sw.js".to_string()]
    );
    // A bare name is page-relative too, and an already-absolute path is left alone.
    assert_eq!(
        refs("navigator.serviceWorker.register(\"sw.js\");\n"),
        vec!["/sw.js".to_string()]
    );
    assert_eq!(
        refs("navigator.serviceWorker.register(\"/service-worker.js\", { scope: \"/\" });\n"),
        vec!["/service-worker.js".to_string()]
    );
}

#[test]
fn register_on_a_non_service_worker_receiver_and_an_unplaceable_path_are_both_skipped() {
    // The receiver gate: `.register` is an extremely common method name, and a plugin registry or a
    // DI container calling it is not an asset load. Same discipline `.addModule` gets.
    assert!(refs("app.register(\"./plugin.js\");\n").is_empty());
    assert!(refs("container.register(\"./service.js\");\n").is_empty());
    // never-guess on the PATH side: another origin's file has nothing in this tree to bump, and a
    // page-relative `..` depends on the page's own served path, which this pass does not know — so
    // it is skipped rather than resolved to the wrong file and silently exempting it.
    assert!(
        refs("navigator.serviceWorker.register(\"https://cdn.example.com/sw.js\");\n").is_empty()
    );
    assert!(refs("navigator.serviceWorker.register(\"../sw.js\");\n").is_empty());
    assert!(refs("navigator.serviceWorker.register(swPath);\n").is_empty());
}

#[test]
fn an_optional_chained_receiver_is_still_the_receiver() {
    // `navigator?.serviceWorker` is an `OptChain`, not a `Member`, and requiring the bare node was a
    // hole in exactly the code most likely to write it: SSR and universal bundles guard `navigator`
    // itself because it does not exist on the server. Same for `ctx?.audioWorklet`.
    assert_eq!(
        refs("navigator?.serviceWorker.register(\"./sw.js\");\n"),
        vec!["/sw.js".to_string()]
    );
    assert_eq!(
        refs("navigator?.serviceWorker?.register(\"./sw.js\");\n"),
        vec!["/sw.js".to_string()]
    );
    assert_eq!(
        refs("await ctx?.audioWorklet.addModule(\"/noise/w.js\");\n"),
        vec!["/noise/w.js".to_string()]
    );
}

#[test]
fn a_scheme_is_not_a_file_reference_and_is_never_rebased() {
    // Rewriting `data:text/javascript,…` to `/data:text/javascript,…` would hand the engine a
    // served-ABSOLUTE string, routing it around the `data:`/`blob:` skip its resolver applies to bare
    // specifiers. Harmless today because nothing matches — but a guard defeated is not a guard.
    assert!(
        refs("navigator.serviceWorker.register(\"data:text/javascript,void 0\");\n").is_empty()
    );
    assert!(refs("navigator.serviceWorker.register(\"blob:https://x/y\");\n").is_empty());
    assert!(
        refs("navigator.serviceWorker.register(\"https://cdn.example.com/sw.js\");\n").is_empty()
    );
    // A COLON after the first slash is a path character, not a scheme — that path is still rebased.
    assert_eq!(
        refs("navigator.serviceWorker.register(\"/a/b:c.js\");\n"),
        vec!["/a/b:c.js".to_string()]
    );
}
