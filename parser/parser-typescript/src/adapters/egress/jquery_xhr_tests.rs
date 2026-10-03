//! `jquery_xhr` coverage — the pre-module-era egress shapes: the positional `$.get`/`$.post`/
//! `$.getJSON` helpers, `$.ajax` in both of its argument forms with both verb spellings, and
//! `xhr.open(verb, url)` where the verb AND the url sit where no other recognized client puts them.
//!
//! Driven through `extract_http_egress` rather than the matcher function, same as `matchers_tests.rs`
//! beside it: a matcher that is never reached still passes its own unit tests, so every assertion here
//! also covers the `.or_else` link in `collector.rs`.

use crate::adapters::egress::{clients, extract_http_egress, files, keys};

fn k(src: &str) -> Vec<Option<String>> {
    keys(&extract_http_egress(&files(&[("a.js", src)])))
}

#[test]
fn positional_jquery_helpers_key_on_their_own_name() {
    let out = extract_http_egress(&files(&[(
        "a.js",
        "$.get('/a');\n$.post('/b');\n$.getJSON('/c');\njQuery.get('/d');\n",
    )]));
    assert_eq!(
        keys(&out),
        vec![
            Some("GET /a".to_string()),
            Some("POST /b".to_string()),
            // `$.getJSON` is `$.get` with a dataType — same verb.
            Some("GET /c".to_string()),
            Some("GET /d".to_string()),
        ]
    );
    assert_eq!(clients(&out), vec![Some("jquery".to_string()); 4]);
}

/// `type:` is jQuery's pre-1.9 spelling of `method:` and must read the same.
#[test]
fn ajax_reads_the_url_property_and_both_verb_spellings() {
    assert_eq!(
        k("$.ajax({ url: '/a', type: 'POST' });"),
        vec![Some("POST /a".to_string())]
    );
    assert_eq!(
        k("$.ajax({ url: '/b', method: 'PUT' });"),
        vec![Some("PUT /b".to_string())]
    );
}

/// jQuery's own precedence when a settings object carries both spellings.
#[test]
fn ajax_lets_method_win_over_type() {
    assert_eq!(
        k("$.ajax({ url: '/a', type: 'GET', method: 'DELETE' });"),
        vec![Some("DELETE /a".to_string())]
    );
}

/// An ABSENT verb is jQuery's documented default, not a guess — the same standing the `fetch` arm's
/// spec-default GET has.
#[test]
fn ajax_without_a_verb_is_the_documented_get_default() {
    assert_eq!(
        k("$.ajax({ url: '/a' });"),
        vec![Some("GET /a".to_string())]
    );
}

/// The two-argument form jQuery has accepted since 1.5.
#[test]
fn ajax_accepts_the_positional_url_form() {
    assert_eq!(
        k("$.ajax('/a', { type: 'POST' });"),
        vec![Some("POST /a".to_string())]
    );
}

/// Under-report, never mis-key. A settings object that STATES a verb without showing its value, or
/// hides one behind a spread or an identifier, drops the whole call site: defaulting it would invent
/// a consume no route provides AND erase the real one.
#[test]
fn ajax_drops_the_site_rather_than_defaulting_a_hidden_verb() {
    assert!(k("$.ajax({ url: '/a', type: verb });").is_empty());
    assert!(k("$.ajax({ url: '/a', ...cfg });").is_empty());
    assert!(k("$.ajax(settings);").is_empty());
    assert!(k("$.ajax('/a', cfg);").is_empty());
}

/// A verb outside the known set cannot mint a channel nobody provides.
#[test]
fn ajax_refuses_a_verb_outside_the_known_set() {
    assert!(k("$.ajax({ url: '/a', type: 'FROBNICATE' });").is_empty());
}

/// A settings object with no `url:` has nothing to key — the request target is elsewhere (a
/// `$.ajaxSetup` default, a plugin), which this pass cannot see.
#[test]
fn ajax_without_a_url_property_emits_nothing() {
    assert!(k("$.ajax({ type: 'POST', data: d });").is_empty());
}

#[test]
fn xhr_open_reads_the_verb_from_the_first_argument_and_the_url_from_the_second() {
    let out = extract_http_egress(&files(&[(
        "a.js",
        "var xhr = new XMLHttpRequest();\nxhr.open('POST', '/a');\nreq.open('GET', '/b');\n",
    )]));
    assert_eq!(
        keys(&out),
        vec![Some("POST /a".to_string()), Some("GET /b".to_string())]
    );
    assert_eq!(clients(&out), vec![Some("xhr".to_string()); 2]);
}

/// The literal verb in the FIRST argument is the whole evidence for this arm, so every shape that
/// lacks it must stay unmatched — `window.open` being the one every browser tree carries.
#[test]
fn open_without_a_literal_verb_is_not_an_xhr_call() {
    assert!(k("window.open('/a', '_blank');").is_empty());
    assert!(k("xhr.open(verb, '/a');").is_empty());
    assert!(k("xhr.open('POST');").is_empty());
}

/// The new arm sits LAST in the collector chain, so a module-era client keys exactly as before.
#[test]
fn a_modern_client_still_keys_the_way_it_did() {
    let out = extract_http_egress(&files(&[(
        "a.js",
        "axios.post('/a', body);\nfetch('/b');\n",
    )]));
    assert_eq!(
        keys(&out),
        vec![Some("POST /a".to_string()), Some("GET /b".to_string())]
    );
    assert_eq!(
        clients(&out),
        vec![Some("axios".to_string()), Some("fetch".to_string())]
    );
}
