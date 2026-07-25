//! `url_resolve` coverage — conditional-literal fan-out (`cond-literal-fanout-v1`) and the template-head
//! same-file constant (`same-file-const-prepend-v1`), including that rule's four negative gates. Split out
//! of `url_resolve.rs` because the pair would exceed the 300-line file budget.

use crate::adapters::egress::{extract_http_egress, files, keys};

// --- conditional-literal fan-out (`cond-literal-fanout-v1`) ---

#[test]
fn template_conditional_literal_interpolation_fans_out_the_url() {
    let out = extract_http_egress(&files(&[(
        "conduit.ts",
        "axios.post(`/users${isRegister ? '' : '/login'}`, body);",
    )]));
    assert_eq!(
        keys(&out),
        vec![
            Some("POST /users".to_string()),
            Some("POST /users/login".to_string()),
        ]
    );
    assert!(out.iter().all(|c| c.raw.is_none() && c.method.is_none()));
}

#[test]
fn top_level_ternary_url_argument_fans_out() {
    let out = extract_http_egress(&files(&[("a.ts", "axios.get(cond ? '/a' : '/b');")]));
    assert_eq!(
        keys(&out),
        vec![Some("GET /a".to_string()), Some("GET /b".to_string())]
    );
}

#[test]
fn template_ternary_with_a_template_arm_keeps_the_in_branch_slash() {
    // fe-vite Editor.jsx shape: the slash lives INSIDE the cons branch (`` `/${slug}` ``). It used to
    // collapse to a malformed `/articles{}`; now the template arm resolves to `/{}` and fans out,
    // keeping the slash — `/articles/{}` (has-slug) and `/articles` (no-slug).
    let out = extract_http_egress(&files(&[(
        "a.jsx",
        "axios.put(`/articles${slug ? `/${slug}` : ''}`);",
    )]));
    assert_eq!(
        keys(&out),
        vec![
            Some("PUT /articles/{}".to_string()),
            Some("PUT /articles".to_string()),
        ]
    );
}

#[test]
fn correlated_verb_and_url_ternaries_pair_instead_of_cross_producting() {
    // fe-vite Editor.jsx: the verb ternary and the url ternary share the SAME guard `slug`, so only
    // the two reachable branches exist — PUT /articles/{} (slug truthy) and POST /articles (falsy).
    // The two cross combos (POST /articles/{}, PUT /articles) are unreachable and must NOT be emitted
    // (they used to cascade into spurious method-mismatch findings).
    let out = extract_http_egress(&files(&[(
        "Editor.jsx",
        "axios[slug ? 'put' : 'post'](`/articles${slug ? `/${slug}` : ''}`, { article });",
    )]));
    assert_eq!(
        keys(&out),
        vec![
            Some("PUT /articles/{}".to_string()),
            Some("POST /articles".to_string()),
        ]
    );
}

#[test]
fn independent_verb_and_url_ternaries_still_cross_product() {
    // DIFFERENT guards (`a` vs `b`) are genuinely independent — the full Cartesian product is the
    // correct over-approximation, unchanged by the correlation special-case.
    let out = extract_http_egress(&files(&[(
        "a.ts",
        "axios[a ? 'put' : 'post'](`/x${b ? '/1' : '/2'}`);",
    )]));
    assert_eq!(out.len(), 4);
}

#[test]
fn top_level_ternary_with_a_template_arm_fans_out() {
    let out = extract_http_egress(&files(&[("a.ts", "axios.get(cond ? `/a/${x}` : '/b');")]));
    assert_eq!(
        keys(&out),
        vec![Some("GET /a/{}".to_string()), Some("GET /b".to_string())]
    );
}

#[test]
fn template_ternary_interpolation_with_one_non_literal_arm_keeps_the_old_placeholder() {
    let out = extract_http_egress(&files(&[("a.ts", "axios.get(`/x${cond ? a : 'b'}`);")]));
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key.as_deref(), Some("GET /x{}"));
}

#[test]
fn more_than_two_conditional_literal_interpolations_falls_back_to_placeholders_for_all() {
    let out = extract_http_egress(&files(&[(
        "a.ts",
        "axios.get(`/x${a ? '1' : '2'}/y${b ? '3' : '4'}/z${c ? '5' : '6'}`);",
    )]));
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key.as_deref(), Some("GET /x{}/y{}/z{}"));
}

#[test]
fn top_level_ternary_with_identical_arms_dedups_to_one_consume() {
    let out = extract_http_egress(&files(&[("a.ts", "axios.get(cond ? '/same' : '/same');")]));
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key.as_deref(), Some("GET /same"));
}

#[test]
fn ternary_with_one_keying_and_one_vetoed_arm_emits_the_key_plus_an_unresolved_consume() {
    // Mixed partial-veto: '/feed' keys, '?public' veto-lists out of every bucket (query-only URL
    // names no path). The keyed variant is emitted AND the vetoed variant falls back to the
    // unresolved shape (raw = the whole ternary's source text, method carried) — strictly additive
    // over the pre-fanout behavior, which emitted only the unresolved consume.
    let out = extract_http_egress(&files(&[(
        "a.ts",
        "axios.get(cond ? '/feed' : '?public');",
    )]));
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].key.as_deref(), Some("GET /feed"));
    assert!(out[0].raw.is_none());
    assert!(out[1].key.is_none());
    assert_eq!(out[1].raw.as_deref(), Some("cond ? '/feed' : '?public'"));
    assert_eq!(out[1].method.as_deref(), Some("GET"));
}

// --- generated-SDK receivers are not recognized (decision: generated SDKs are injection
// adapters, not engine vocab — the former oazapfts-specific recognition lived here) ---

#[test]
fn former_qs_suffix_special_case_is_gone_trailing_interpolation_is_a_plain_placeholder() {
    // A trailing `${QS.query(...)}`-shaped interpolation used to be dropped entirely as
    // oazapfts-codegen's query-string suffix. That special case is gone: it now keys like any other
    // trailing interpolation, as an ordinary `{}` placeholder.
    let out = extract_http_egress(&files(&[(
        "activity.ts",
        r#"axios.get(`/activities${QS.query(QS.explode({ albumId }))}`);"#,
    )]));
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key.as_deref(), Some("GET /activities{}"));
}

// --- same-file top-level literal const at the template HEAD (`same-file-const-prepend-v1`) ---

#[test]
fn same_file_literal_const_head_resolves_and_reclassifies_the_call_as_external() {
    // The driving case (mono-hub life-hub-fe `joke-generator/fetchJoke.ts`): the whole ORIGIN lives in a
    // same-file top-level literal const. Before, `${BASE}` became `{}` and `consume_key_for`'s
    // base-carrier head-drop threw the host away, leaving `GET /{}` — an all-placeholder key that is not
    // a route identity, so the call landed in the unresolved bucket with its third-party host lost. Now
    // the head is READ, the URL classifies as external, and the call is filed as the third-party egress
    // it visibly is.
    let out = extract_http_egress(&files(&[(
        "fetchJoke.ts",
        r#"const BASE = "https://v2.jokeapi.dev/joke";
export async function fetchJoke(category: string) {
  return fetch(`${BASE}/${category}?safe-mode`);
}"#,
    )]));
    assert_eq!(out.len(), 1);
    assert_eq!(
        out[0].key.as_deref(),
        Some("GET https://v2.jokeapi.dev/joke/{}?safe-mode")
    );
    assert!(out[0].raw.is_none());
}

#[test]
fn same_file_literal_const_head_keys_an_internal_prefix() {
    // The internal counterpart: the head carries a path prefix, so the key gains the `/api` dimension
    // that head-drop used to discard.
    let out = extract_http_egress(&files(&[(
        "a.ts",
        "const API = '/api'; axios.get(`${API}/users/${id}`);",
    )]));
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key.as_deref(), Some("GET /api/users/{}"));
}

#[test]
fn same_file_literal_const_is_only_read_at_the_head_not_mid_path() {
    // Positional gate: a mid-path interpolation is a route PARAMETER slot and `{}` is its correct
    // normalization — substituting `v1` there would stop the call joining the `:version` route it
    // belongs to. Only the head, where `{}` is a loss rather than a normalization, is read.
    let out = extract_http_egress(&files(&[(
        "a.ts",
        "const V = 'v1'; axios.get(`/api/${V}/users`);",
    )]));
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key.as_deref(), Some("GET /api/{}/users"));
}

// --- the four never-guess gates, one negative fixture each ---

#[test]
fn gate1_negative_a_second_binding_of_the_name_is_never_resolved() {
    // Gate 1 (single binding): the file declares the name twice — a block-scoped redeclaration alongside
    // the top-level one. Which declaration reaches the call site is a scope question, and the answer is
    // to refuse it: the name is dropped and the old head-drop behavior (base thrown away, visible literal
    // keys the call) stands.
    let out = extract_http_egress(&files(&[(
        "a.ts",
        "const BASE = 'https://a.example.com'; { const BASE = 'https://b.example.com'; }\naxios.get(`${BASE}/users`);",
    )]));
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key.as_deref(), Some("GET /users"));
}

#[test]
fn gate2_negative_a_reassigned_let_is_never_resolved() {
    // Gate 2 (no reassignment): a `let` whose value is overwritten later is not a constant. Resolving its
    // FIRST value would be a guess about which assignment reached the call site.
    let out = extract_http_egress(&files(&[(
        "a.ts",
        "let BASE = 'https://a.example.com'; BASE = 'https://b.example.com';\naxios.get(`${BASE}/users`);",
    )]));
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key.as_deref(), Some("GET /users"));
    // Same for the compound/update forms, which never reach `visit_assign_expr`'s simple-ident arm by
    // the same syntax.
    let out = extract_http_egress(&files(&[(
        "b.ts",
        "let BASE = 'https://a.example.com'; BASE += '/v2';\naxios.get(`${BASE}/users`);",
    )]));
    assert_eq!(out[0].key.as_deref(), Some("GET /users"));
}

#[test]
fn gate3_negative_a_parameter_shadowing_the_name_is_never_resolved() {
    // Gate 3 (no param shadow): inside `send`, `BASE` is the PARAMETER, not the module constant. A
    // scope-insensitive map would key this call with the module constant's value — exactly the mis-keying
    // the project-wide const map refuses bare names to avoid.
    let out = extract_http_egress(&files(&[(
        "a.ts",
        "const BASE = 'https://module.example.com';\nfunction send(BASE) { return axios.get(`${BASE}/users`); }",
    )]));
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key.as_deref(), Some("GET /users"));
}

#[test]
fn gate3_negative_a_destructured_or_arrow_binding_shadow_is_never_resolved() {
    // Gate 3, the two shadow forms most easily missed by a hand-written parameter walk: a destructuring
    // binding and an arrow parameter. The binding CENSUS catches both because each is a binding
    // occurrence, no matter which syntax produced it.
    let out = extract_http_egress(&files(&[(
        "a.ts",
        "const BASE = 'https://module.example.com';\nfunction f(o) { const { BASE } = o; return axios.get(`${BASE}/users`); }",
    )]));
    assert_eq!(out[0].key.as_deref(), Some("GET /users"));
    let out = extract_http_egress(&files(&[(
        "b.ts",
        "const BASE = 'https://module.example.com';\nconst g = (BASE) => axios.get(`${BASE}/users`);",
    )]));
    assert_eq!(out[0].key.as_deref(), Some("GET /users"));
}

#[test]
fn gate4_negative_a_non_literal_initializer_is_never_resolved() {
    // Gate 4 (literal only): an env read, a call, another identifier, and an interpolated template are all
    // values this file does NOT write down. Each stays opaque — the head-drop residue, not a guess.
    for src in [
        "const BASE = process.env.API_URL;",
        "const BASE = makeBase();",
        "const BASE = OTHER;",
        "const BASE = `${proto}://host`;",
        "const BASE = 'https://a.example.com' + suffix;",
    ] {
        let out = extract_http_egress(&files(&[(
            "a.ts",
            &format!("{src}\naxios.get(`${{BASE}}/users`);"),
        )]));
        assert_eq!(out.len(), 1, "{src}");
        assert_eq!(out[0].key.as_deref(), Some("GET /users"), "{src}");
    }
}

// --- shape refusals: reading the head must not smuggle a call past the head-drop bucket's own
// never-guess vetoes (`keying.rs`'s `{}{}`-head / non-`/` suffix / `//`-host pins) ---

#[test]
fn head_is_not_read_when_a_dynamic_piece_follows_it_immediately() {
    // Before the shape gate this keyed `GET /api{}`. In this key vocabulary `{}` is ONE WHOLE path-param
    // segment, so `api{}` asserts a segment literally spelled `api<something>` — a route nobody wrote,
    // and one that PASSES `key_carries_route_identity`, so the linker would file it as an
    // `unprovidedConsumes` drift finding instead of the honest silence it had before. This is exactly the
    // shape `base_relative_veto_list_still_never_keys` pins as never-guess; reading the head must not
    // reverse that decision.
    let out = extract_http_egress(&files(&[(
        "a.ts",
        "const BASE = '/api'; axios.get(`${BASE}${path}`);",
    )]));
    assert_eq!(out.len(), 1);
    assert!(out[0].key.is_none(), "got: {:?}", out[0].key);
    assert_eq!(out[0].raw.as_deref(), Some("`${BASE}${path}`"));
}

#[test]
fn head_is_not_read_when_the_following_slots_are_capped_conditionals() {
    // Same gate, reached the other way: 3+ conditional slots blow the fan-out cap and ALL of them fall
    // back to `{}`, so the piece after the head is dynamic again. Used to key `GET /api{}{}{}`.
    let out = extract_http_egress(&files(&[(
        "a.ts",
        "const B = '/api'; axios.get(`${B}${a ? '/x' : '/y'}${b ? '1' : '2'}${c ? '3' : '4'}`);",
    )]));
    assert_eq!(out.len(), 1);
    assert!(out[0].key.is_none(), "got: {:?}", out[0].key);
}

#[test]
fn a_protocol_relative_head_value_is_never_read() {
    // `const CDN = '//cdn.example.com'` is a real idiom. Substituted, the URL is `/`-headed, so it keys
    // INTERNAL and `normalize_http_path` collapses the `//` — turning a third-party HOST into a path
    // SEGMENT (`GET /cdn.example.com/x`). Refused; the call keeps the old head-drop residue.
    let out = extract_http_egress(&files(&[(
        "a.ts",
        "const CDN = '//cdn.example.com'; axios.get(`${CDN}/x`);",
    )]));
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key.as_deref(), Some("GET /x"));
}

#[test]
fn a_head_value_with_a_trailing_slash_is_read_anyway_and_keys_the_literal_truth() {
    // Deliberately NOT refused. `https://x.com/` + `/users` is what the call literally requests, and the
    // key stays in the external bucket where nothing joins — the cost is a duplicate spelling. Refusing
    // would fall back to head-drop and key `GET /users`, a FALSE INTERNAL claim that can join a
    // same-named local route. Between a cosmetic double slash and a wrong edge, this is the lesser harm.
    let out = extract_http_egress(&files(&[(
        "a.ts",
        "const B = 'https://x.com/'; axios.get(`${B}/users`);",
    )]));
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key.as_deref(), Some("GET https://x.com//users"));
}

#[test]
fn a_non_slash_remainder_is_read_because_a_known_head_leaves_no_ambiguity() {
    // The head-drop bucket refuses `{}users` because with an OPAQUE head the segment boundary is
    // invisible. Once the head is READ there is no boundary question left: `/api` + `users` is exactly
    // `/apiusers`, and keying the truth is not a guess. This is why the shape gate asks only whether the
    // next piece is LITERAL, not whether it starts with `/`.
    let out = extract_http_egress(&files(&[(
        "a.ts",
        "const BASE = '/api'; axios.get(`${BASE}users`);",
    )]));
    assert_eq!(out[0].key.as_deref(), Some("GET /apiusers"));
}

#[test]
fn a_query_headed_remainder_is_read_the_air_quality_shape() {
    // mono-hub `fetchAirQuality.ts`: the remainder starts with `?`, not `/`. A known head makes that
    // unambiguous, and the external branch carries the query verbatim as it always has.
    let out = extract_http_egress(&files(&[(
        "a.ts",
        "const AQ = 'https://aq.example.com/v1/air-quality'; fetch(`${AQ}?lat=${lat}`);",
    )]));
    assert_eq!(
        out[0].key.as_deref(),
        Some("GET https://aq.example.com/v1/air-quality?lat={}")
    );
}

#[test]
fn template_and_concat_answer_identically_for_every_head_shape() {
    // The isomorphism `concat.rs` claims in prose, pinned. Each pair must produce the SAME key — the
    // deviation the shape gate closed was `${BASE}${path}` keying while `BASE + path` stayed unresolved.
    for (tpl, cat) in [
        ("`${BASE}/users`", "BASE + '/users'"),
        ("`${BASE}users`", "BASE + 'users'"),
        ("`${BASE}${path}`", "BASE + path"),
        ("`${BASE}/x/${id}`", "BASE + '/x/' + id"),
    ] {
        let t = extract_http_egress(&files(&[(
            "t.ts",
            &format!("const BASE = '/api'; axios.get({tpl});"),
        )]));
        let c = extract_http_egress(&files(&[(
            "c.ts",
            &format!("const BASE = '/api'; axios.get({cat});"),
        )]));
        assert_eq!(keys(&t), keys(&c), "{tpl} vs {cat}");
    }
}

// --- binding census: TypeScript's value-namespace declaration forms also shadow ---

#[test]
fn a_typescript_value_declaration_shadowing_the_name_is_never_resolved() {
    // `enum`, `namespace`, and `import X = require(...)` all bind a VALUE and can shadow a constant. Any
    // of them left out of the census would be precisely the "a binding form the census never enumerated"
    // failure `local_consts`'s module doc claims is impossible. Type-only `interface`/`type` bind no
    // value and are correctly absent — covered by the positive case below.
    for shadow in [
        "function f() { enum BASE { A } return axios.get(`${BASE}/users`); }",
        "namespace BASE { export const x = 1; }\naxios.get(`${BASE}/users`);",
        "import BASE = require('x');\naxios.get(`${BASE}/users`);",
    ] {
        let out = extract_http_egress(&files(&[(
            "a.ts",
            &format!("const BASE = '/api';\n{shadow}"),
        )]));
        assert_eq!(out[0].key.as_deref(), Some("GET /users"), "{shadow}");
    }
    // A type-only declaration of the same name does NOT shadow the value, so the head still reads.
    let out = extract_http_egress(&files(&[(
        "b.ts",
        "const BASE = '/api'; interface BASE { x: string }\naxios.get(`${BASE}/users`);",
    )]));
    assert_eq!(out[0].key.as_deref(), Some("GET /api/users"));
}

// --- the WHOLE-ARGUMENT position (`same-file-url-binding-v1`) ---

#[test]
fn whole_argument_bare_const_resolves_the_third_party_url() {
    // Seals the measured C1 class: mono-hub community-hub-fe has THIRTEEN source adapters shaped exactly
    // like this, each with its own `const URL`. Before, `fetch(URL)` carried `raw = "URL"` and landed in
    // the unresolved bucket, so a tree whose egress is entirely third-party reported 100% unresolved —
    // not because the code was dynamic but because the literal three lines down was never read.
    let out = extract_http_egress(&files(&[(
        "REDDIT_POPULAR.ts",
        r#"export const REDDIT = { fetchHotPosts: async () => fetch(URL, { headers: {} }) };
const URL = "https://www.reddit.com/r/popular.json?limit=50";"#,
    )]));
    assert_eq!(out.len(), 1);
    assert_eq!(
        out[0].key.as_deref(),
        Some("GET https://www.reddit.com/r/popular.json?limit=50")
    );
    assert!(out[0].raw.is_none());
}

#[test]
fn the_same_constant_answers_identically_at_both_positions() {
    // The inconsistency that drove this rule (mono-hub tool-hub-fe `runMeasurement.ts`): ONE file, ONE
    // `const BASE`, read at the template head (external) but not as the whole argument (unresolved). Same
    // fact, two answers — the position-axis twin of the bucket-vs-rule double surface. Pinned as a pair so
    // neither position can regress alone.
    let out = extract_http_egress(&files(&[(
        "runMeasurement.ts",
        r#"export async function run(id: string) {
  await fetch(BASE, { method: "POST", body: "{}" });
  return fetch(`${BASE}/${id}`);
}
const BASE = "https://api.globalping.io/v1/measurements";"#,
    )]));
    assert_eq!(
        keys(&out),
        vec![
            Some("POST https://api.globalping.io/v1/measurements".to_string()),
            Some("GET https://api.globalping.io/v1/measurements/{}".to_string()),
        ]
    );
}

#[test]
fn function_local_url_variable_resolves_one_hop() {
    // The measured C2 class: `const url = <resolvable>; fetch(url)`. NESTING IS NOT A GATE — the binding
    // census proves the name is bound exactly once in the file, which is what carries the scope argument,
    // so a function-local declarator qualifies exactly like a top-level one.
    let out = extract_http_egress(&files(&[(
        "a.ts",
        r#"export async function load(q: string) {
  const url = `https://api.datamuse.com/words?rel_syn=${q}&max=10`;
  return fetch(url);
}"#,
    )]));
    assert_eq!(out.len(), 1);
    // The external branch carries the query verbatim (query-drop is the INTERNAL key's normalization) —
    // pinned here so this rule's output stays the literal truth of what the call requests.
    assert_eq!(
        out[0].key.as_deref(),
        Some("GET https://api.datamuse.com/words?rel_syn={}&max=10")
    );
}

#[test]
fn function_local_url_variable_reads_a_same_file_base_at_its_head() {
    // The two same-file rules COMPOSE: the head rule resolves `${AQ}` inside the initializer, and the
    // whole-argument rule then reads the assembled value at `fetch(url)`. This is the shape behind the
    // measured C2b sub-class, and it is exactly one hop — the initializer resolved on its own.
    let out = extract_http_egress(&files(&[(
        "fetchAirQuality.ts",
        r#"const AQ = "https://air-quality-api.open-meteo.com/v1/air-quality";
export async function get(lat: number) {
  const url = `${AQ}?latitude=${lat}`;
  return fetch(url);
}"#,
    )]));
    assert_eq!(out.len(), 1);
    assert_eq!(
        out[0].key.as_deref(),
        Some("GET https://air-quality-api.open-meteo.com/v1/air-quality?latitude={}")
    );
}

#[test]
fn whole_argument_resolution_stops_at_one_hop() {
    // never-guess boundary: ONE hop, and only when the initializer resolves BY ITSELF. `b` is defined in
    // terms of another candidate, which the empty-`urls` build order refuses structurally rather than by a
    // hop counter. Silence, not a chased chain.
    let out = extract_http_egress(&files(&[(
        "a.ts",
        "const a = 'https://x.example.com/v1'; const b = a; fetch(b);",
    )]));
    assert_eq!(out.len(), 1);
    assert!(out[0].key.is_none(), "got: {:?}", out[0].key);
    assert_eq!(out[0].raw.as_deref(), Some("b"));
}

#[test]
fn a_wrapper_parameter_is_never_read_at_the_whole_argument() {
    // never-guess boundary: interprocedural VALUE resolution is a different body of work (and a quality bar
    // this parser has already declined once). `url` here is a parameter, not a declarator — it is never a
    // candidate, no matter that it is bound exactly once.
    let out = extract_http_egress(&files(&[(
        "a.ts",
        "export function fetchJson(url: string) { return fetch(url); }",
    )]));
    assert_eq!(out.len(), 1);
    assert!(out[0].key.is_none());
    assert_eq!(out[0].raw.as_deref(), Some("url"));
}

#[test]
fn an_env_or_computed_binding_is_never_read_at_the_whole_argument() {
    // never-guess boundary: an env read and a call are values this FILE does not write down — their value
    // is an environment/runtime fact and enters by injection, not inference. The shape looks static; the
    // value is not.
    for src in [
        "const base = process.env.API_URL;",
        "const base = apiBase();",
        "const base = cfg.feedUrl;",
    ] {
        let out = extract_http_egress(&files(&[("a.ts", &format!("{src}\nfetch(base);"))]));
        assert_eq!(out.len(), 1, "{src}");
        assert!(out[0].key.is_none(), "{src} -> {:?}", out[0].key);
        assert_eq!(out[0].raw.as_deref(), Some("base"), "{src}");
    }
}

#[test]
fn whole_argument_binding_obeys_the_same_census_gates_as_the_head() {
    // The gates are ONE predicate shared by both positions (`binding_census`), not two hand-written lists
    // that can drift. Each fixture breaks a different gate and must fall back to today's unresolved
    // consume with `raw` carried — a second binding, a parameter shadow, and a reassigned `let`.
    for src in [
        "const U = 'https://a.example.com/x'; { const U = 'https://b.example.com/x'; }\nfetch(U);",
        "const U = 'https://a.example.com/x';\nexport function send(U) { return fetch(U); }",
        "let U = 'https://a.example.com/x'; U = 'https://b.example.com/x';\nfetch(U);",
    ] {
        let out = extract_http_egress(&files(&[("a.ts", src)]));
        assert_eq!(out.len(), 1, "{src}");
        assert!(out[0].key.is_none(), "{src} -> {:?}", out[0].key);
        assert_eq!(out[0].raw.as_deref(), Some("U"), "{src}");
    }
}

#[test]
fn a_protocol_relative_binding_is_never_read_at_the_whole_argument_either() {
    // Class sweep of `a_protocol_relative_head_value_is_never_read`: the SAME harm reaches the new
    // position. `//cdn.example.com/x` is `/`-headed, so it keys internal and `normalize_http_path`
    // collapses the `//` — a third-party HOST filed as an internal path SEGMENT. The refusal lives once,
    // at map construction, so both positions cannot answer it differently.
    let out = extract_http_egress(&files(&[(
        "a.ts",
        "const CDN = '//cdn.example.com/x'; fetch(CDN);",
    )]));
    assert_eq!(out.len(), 1);
    assert!(out[0].key.is_none(), "got: {:?}", out[0].key);
    assert_eq!(out[0].raw.as_deref(), Some("CDN"));
}

#[test]
fn a_computed_member_url_binding_is_still_not_read() {
    // never-guess boundary: a partial resolution stays partial. `ENDPOINT[kind]` names an object-literal
    // entry through a DYNAMIC key — the honest answers are "enumerate both" or "unresolved", never "probably
    // the first one". Only a BARE identifier is read at this position.
    let out = extract_http_egress(&files(&[(
        "a.ts",
        "const ENDPOINT = { ping: 'https://a.example.com', trace: 'https://b.example.com' };\nfetch(ENDPOINT[kind]);",
    )]));
    assert_eq!(out.len(), 1);
    assert!(out[0].key.is_none(), "got: {:?}", out[0].key);
}

#[test]
fn a_literal_const_from_another_file_is_never_resolved() {
    // Scope gate: the map is built per file. A same-named top-level const in a sibling file is not this
    // file's fact — cross-file constant promotion is a separate body of work, not this rule.
    let out = extract_http_egress(&files(&[
        ("base.ts", "export const BASE = 'https://a.example.com';"),
        ("call.ts", "axios.get(`${BASE}/users`);"),
    ]));
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].key.as_deref(), Some("GET /users"));
}
