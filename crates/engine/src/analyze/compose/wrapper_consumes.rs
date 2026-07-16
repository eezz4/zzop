use std::collections::HashMap;

use zzop_core::IoConsume;

/// Join per-file wrapper CALL fragments against wrapper DEFINITION fragments and emit an `http`
/// `IoConsume` at each resolvable CALL site — the consume-side twin of the provide composers: the
/// wrapper's own body only ever shows egress a non-literal sink (`axios.request(options)`), so
/// without this join a project-local request-wrapper family is invisible and every consume anchor
/// points at wrapper internals instead of the code a reader would edit.
///
/// Resolution: a call's `callee` finds its def in the SAME file first (local wrapper), else via
/// `resolve(specifier, from_file)` → that file's def of the same name (the same workspace-aware
/// resolver the provide composers use). Method = the def's `fixed_method` or the call's
/// `method_param`-indexed arg (must be a literal GET/POST/PUT/PATCH/DELETE — anything else skips
/// the call, never guesses); path = the `path_param`-indexed arg (must start with `/`). Emitted
/// consumes are fully keyed (no late resolution) and deduped/sorted deterministically.
pub(crate) fn resolve_wrapper_consumes(
    def_pairs: Vec<(String, Vec<zzop_core::WrapperDefFragment>)>,
    call_pairs: Vec<(String, Vec<zzop_core::WrapperCallFragment>)>,
    resolve: impl Fn(&str, &str) -> Option<String>,
    io_consumes: &mut Vec<IoConsume>,
) {
    let mut defs: HashMap<(String, String), &zzop_core::WrapperDefFragment> = HashMap::new();
    for (file, frags) in &def_pairs {
        for def in frags {
            defs.insert((file.clone(), def.name.clone()), def);
        }
    }

    let mut call_pairs = call_pairs;
    call_pairs.sort_by(|a, b| a.0.cmp(&b.0));

    let mut out: Vec<IoConsume> = Vec::new();
    for (file, calls) in &call_pairs {
        for call in calls {
            let def_file = match &call.specifier {
                None => Some(file.clone()),
                Some(spec) => resolve(spec, file),
            };
            let def = def_file.and_then(|f| defs.get(&(f, call.callee.clone())).copied());
            let Some(def) = def else { continue };
            let method = match (&def.fixed_method, def.method_param) {
                (Some(m), _) => Some(m.clone()),
                // Any-case verb literal accepted and uppercased — the same tolerance
                // `egress::method_from_options` applies (its own tests use `method: "delete"`).
                (None, Some(idx)) => call
                    .args
                    .get(idx as usize)
                    .and_then(|a| a.clone())
                    .map(|m| m.to_ascii_uppercase())
                    // Verb vocabulary is the core T1 single source, not a local copy (policy census).
                    .filter(|m| zzop_core::HTTP_KEY_VERBS.contains(&m.as_str())),
                (None, None) => None,
            };
            let Some(method) = method else { continue };
            let path = call
                .args
                .get(def.path_param as usize)
                .and_then(|a| a.clone())
                .filter(|p| p.starts_with('/'));
            let Some(path) = path else { continue };
            out.push(IoConsume {
                client: None,
                body: None,
                kind: "http".to_string(),
                key: Some(zzop_core::http_consume_interface_key(&method, &path)),
                file: file.clone(),
                line: call.line,
                raw: None,
                method: None,
            });
        }
    }

    out.sort_by(|a, b| {
        a.key
            .cmp(&b.key)
            .then_with(|| a.file.cmp(&b.file))
            .then_with(|| a.line.cmp(&b.line))
    });
    out.dedup_by(|a, b| a.key == b.key && a.file == b.file && a.line == b.line);
    io_consumes.extend(out);
}

#[cfg(test)]
mod wrapper_consume_tests {
    //! Coverage for `resolve_wrapper_consumes`: cross-file join via specifier, same-file local
    //! wrapper, fixed-method wrappers, the never-guess skips (non-verb method arg, non-`/` path,
    //! unresolvable specifier), and determinism.
    use super::*;
    use zzop_core::{WrapperCallFragment, WrapperDefFragment};

    fn def(
        name: &str,
        method_param: Option<u32>,
        path_param: u32,
        fixed: Option<&str>,
    ) -> WrapperDefFragment {
        WrapperDefFragment {
            name: name.to_string(),
            method_param,
            path_param,
            fixed_method: fixed.map(str::to_string),
        }
    }

    fn call(
        callee: &str,
        specifier: Option<&str>,
        args: Vec<Option<&str>>,
        line: u32,
    ) -> WrapperCallFragment {
        WrapperCallFragment {
            callee: callee.to_string(),
            specifier: specifier.map(str::to_string),
            args: args.into_iter().map(|a| a.map(str::to_string)).collect(),
            line,
        }
    }

    fn resolver<'a>(
        map: &'a [(&'a str, &'a str, &'a str)],
    ) -> impl Fn(&str, &str) -> Option<String> + 'a {
        move |spec: &str, from: &str| {
            map.iter()
                .find(|(s, f, _)| *s == spec && *f == from)
                .map(|(_, _, t)| t.to_string())
        }
    }

    #[test]
    fn imported_wrapper_call_becomes_a_keyed_consume_at_the_call_site() {
        let defs = vec![(
            "utils/api.ts".to_string(),
            vec![def("makeRestApiRequest", Some(1), 2, None)],
        )];
        let calls = vec![(
            "src/api/workflows.ts".to_string(),
            vec![
                call(
                    "makeRestApiRequest",
                    Some("@/utils/api"),
                    vec![None, Some("GET"), Some("/workflows/new")],
                    12,
                ),
                call(
                    "makeRestApiRequest",
                    Some("@/utils/api"),
                    vec![None, Some("POST"), Some("/workflows/{}/activate"), None],
                    30,
                ),
            ],
        )];
        let mut consumes = Vec::new();
        resolve_wrapper_consumes(
            defs,
            calls,
            resolver(&[("@/utils/api", "src/api/workflows.ts", "utils/api.ts")]),
            &mut consumes,
        );
        let keys: Vec<&str> = consumes.iter().flat_map(|c| c.key.as_deref()).collect();
        assert_eq!(
            keys,
            vec!["GET /workflows/new", "POST /workflows/{}/activate"]
        );
        assert_eq!(consumes[0].file, "src/api/workflows.ts");
        assert_eq!(consumes[0].line, 12);
    }

    #[test]
    fn fixed_method_wrapper_and_same_file_local_call() {
        let defs = vec![(
            "src/stream.ts".to_string(),
            vec![def("streamRequest", None, 1, Some("POST"))],
        )];
        let calls = vec![(
            "src/stream.ts".to_string(),
            vec![call(
                "streamRequest",
                None,
                vec![None, Some("/ai/chat")],
                40,
            )],
        )];
        let mut consumes = Vec::new();
        resolve_wrapper_consumes(defs, calls, |_, _| None, &mut consumes);
        assert_eq!(consumes.len(), 1);
        assert_eq!(consumes[0].key.as_deref(), Some("POST /ai/chat"));
    }

    #[test]
    fn never_guesses_on_non_verb_non_path_or_unresolvable() {
        let defs = vec![(
            "utils/api.ts".to_string(),
            vec![def("makeRestApiRequest", Some(1), 2, None)],
        )];
        let calls = vec![(
            "src/a.ts".to_string(),
            vec![
                // method arg is a variable, not a literal verb
                call(
                    "makeRestApiRequest",
                    Some("./u"),
                    vec![None, None, Some("/x")],
                    1,
                ),
                // path arg does not start with '/'
                call(
                    "makeRestApiRequest",
                    Some("./u"),
                    vec![None, Some("GET"), Some("x")],
                    2,
                ),
                // unresolvable specifier
                call(
                    "makeRestApiRequest",
                    Some("./nowhere"),
                    vec![None, Some("GET"), Some("/x")],
                    3,
                ),
            ],
        )];
        let mut consumes = Vec::new();
        resolve_wrapper_consumes(
            defs,
            calls,
            resolver(&[("./u", "src/a.ts", "utils/api.ts")]),
            &mut consumes,
        );
        assert!(consumes.is_empty());
    }

    #[test]
    fn output_is_deterministic_across_input_order() {
        let defs = vec![("u.ts".to_string(), vec![def("w", Some(0), 1, None)])];
        let build = |rev: bool| {
            let mut v = vec![
                (
                    "a.ts".to_string(),
                    vec![call("w", Some("./u"), vec![Some("GET"), Some("/a")], 1)],
                ),
                (
                    "b.ts".to_string(),
                    vec![call("w", Some("./u"), vec![Some("GET"), Some("/b")], 1)],
                ),
            ];
            if rev {
                v.reverse();
            }
            v
        };
        let run = |calls| {
            let mut out = Vec::new();
            resolve_wrapper_consumes(
                defs.clone(),
                calls,
                resolver(&[("./u", "a.ts", "u.ts"), ("./u", "b.ts", "u.ts")]),
                &mut out,
            );
            out.into_iter()
                .map(|c| (c.key, c.file, c.line))
                .collect::<Vec<_>>()
        };
        assert_eq!(run(build(false)), run(build(true)));
    }
}
