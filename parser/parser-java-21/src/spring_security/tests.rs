//! Coverage for `extract_spring_security_posture`: the be-spring `configure(HttpSecurity)` chain, the
//! secure-by-default gate, permitAll enumeration (with/without HttpMethod), and the parse-all-or-nothing
//! safety bails (unrecognized clause, non-literal path, no anyRequest, restriction terminal).

use super::{extract_spring_security_posture, SpringAntMatcher};

// The exact be-spring `WebSecurityConfig.configure` authorization chain (prefix noise trimmed to the
// authorizeRequests region — the extractor ascends from `authorizeRequests`, so the leading
// `.csrf().disable()...` is irrelevant, but included here to prove it's skipped).
const BE_SPRING: &str = r#"
public class WebSecurityConfig {
  protected void configure(HttpSecurity http) throws Exception {
    http.csrf().disable().cors().and()
        .authorizeRequests()
        .antMatchers(HttpMethod.OPTIONS).permitAll()
        .antMatchers("/graphiql").permitAll()
        .antMatchers("/graphql").permitAll()
        .antMatchers(HttpMethod.GET, "/articles/feed").authenticated()
        .antMatchers(HttpMethod.POST, "/users", "/users/login").permitAll()
        .antMatchers(HttpMethod.GET, "/articles/**", "/profiles/**", "/tags").permitAll()
        .anyRequest().authenticated();
  }
}
"#;

#[test]
fn parses_the_be_spring_posture_permit_all_list() {
    let posture = extract_spring_security_posture("WebSecurityConfig.java", BE_SPRING)
        .expect("secure-by-default chain should parse");
    assert_eq!(
        posture.permit_all,
        vec![
            SpringAntMatcher {
                method: Some("OPTIONS".into()),
                patterns: vec![]
            },
            SpringAntMatcher {
                method: None,
                patterns: vec!["/graphiql".into()]
            },
            SpringAntMatcher {
                method: None,
                patterns: vec!["/graphql".into()]
            },
            // the GET /articles/feed `.authenticated()` matcher is recognized but NOT in permit_all
            SpringAntMatcher {
                method: Some("POST".into()),
                patterns: vec!["/users".into(), "/users/login".into()],
            },
            SpringAntMatcher {
                method: Some("GET".into()),
                patterns: vec!["/articles/**".into(), "/profiles/**".into(), "/tags".into()],
            },
        ],
    );
}

#[test]
fn a_chain_without_any_request_authenticated_bails() {
    // No `.anyRequest().authenticated()` -> default posture unknown -> None (safe: no exemption).
    let src = r#"
public class C {
  protected void configure(HttpSecurity http) throws Exception {
    http.authorizeRequests().antMatchers("/x").permitAll();
  }
}
"#;
    assert!(extract_spring_security_posture("C.java", src).is_none());
}

#[test]
fn any_request_permit_all_default_bails() {
    // `.anyRequest().permitAll()` is open-by-default — we cannot infer any route is authenticated.
    let src = r#"
public class C {
  protected void configure(HttpSecurity http) throws Exception {
    http.authorizeRequests().antMatchers("/x").authenticated().anyRequest().permitAll();
  }
}
"#;
    assert!(extract_spring_security_posture("C.java", src).is_none());
}

#[test]
fn an_unrecognized_restriction_terminal_bails() {
    // `.hasRole(...)` is a restriction this v1 doesn't model -> bail (never partially trust the chain).
    let src = r#"
public class C {
  protected void configure(HttpSecurity http) throws Exception {
    http.authorizeRequests()
        .antMatchers("/admin/**").hasRole("ADMIN")
        .anyRequest().authenticated();
  }
}
"#;
    assert!(extract_spring_security_posture("C.java", src).is_none());
}

#[test]
fn a_non_literal_matcher_path_bails() {
    // A variable/constant path can't be reasoned about -> bail rather than guess.
    let src = r#"
public class C {
  protected void configure(HttpSecurity http) throws Exception {
    http.authorizeRequests()
        .antMatchers(PUBLIC_PATH).permitAll()
        .anyRequest().authenticated();
  }
}
"#;
    assert!(extract_spring_security_posture("C.java", src).is_none());
}

#[test]
fn two_authorization_chains_bail() {
    // More than one authorizeRequests chain (multiple SecurityFilterChains) is ambiguous -> bail.
    let src = r#"
public class C {
  protected void configureA(HttpSecurity http) throws Exception {
    http.authorizeRequests().anyRequest().authenticated();
  }
  protected void configureB(HttpSecurity http) throws Exception {
    http.authorizeRequests().anyRequest().permitAll();
  }
}
"#;
    assert!(extract_spring_security_posture("C.java", src).is_none());
}

#[test]
fn a_chain_level_request_scoper_bails() {
    // `http.antMatcher("/api/**").authorizeRequests()...` scopes the WHOLE chain to `/api/**`, so its
    // posture is NOT global — applying it would false-clear open routes outside `/api`. Must return None.
    let src = r#"
public class C {
  protected void configure(HttpSecurity http) throws Exception {
    http.antMatcher("/api/**")
        .authorizeRequests()
        .antMatchers("/api/public").permitAll()
        .anyRequest().authenticated();
  }
}
"#;
    assert!(
        extract_spring_security_posture("C.java", src).is_none(),
        "a chain-level antMatcher scope must bail (not a global posture)"
    );
}

#[test]
fn a_security_matcher_scoper_bails() {
    let src = r#"
public class C {
  protected void configure(HttpSecurity http) throws Exception {
    http.securityMatcher("/admin/**").authorizeHttpRequests().anyRequest().authenticated();
  }
}
"#;
    assert!(extract_spring_security_posture("C.java", src).is_none());
}

#[test]
fn other_chain_level_scopers_regex_and_plural_request_matchers_bail() {
    // The enumeration-free `contains("Matcher")` gate must catch these too (not just the 4 originally
    // listed): legacy `regexMatcher` and the plural `requestMatchers()` chain entrypoint form.
    let regex = r#"
public class C { protected void configure(HttpSecurity http) throws Exception {
  http.regexMatcher("/api/.*").authorizeRequests().antMatchers("/api/public").permitAll().anyRequest().authenticated();
} }
"#;
    assert!(
        extract_spring_security_posture("C.java", regex).is_none(),
        "regexMatcher scope must bail"
    );
    let plural = r#"
public class C { protected void configure(HttpSecurity http) throws Exception {
  http.requestMatchers().antMatchers("/api/**").and().authorizeRequests().anyRequest().authenticated();
} }
"#;
    assert!(
        extract_spring_security_posture("C.java", plural).is_none(),
        "requestMatchers() scope must bail"
    );
}

#[test]
fn a_web_security_ignoring_config_bails() {
    // `WebSecurity.ignoring()` opens paths outside the authorizeRequests chain -> can't fully see the
    // config -> bail rather than risk exempting an ignored (open) mutating route.
    let src = r#"
public class C {
  public void configure(WebSecurity web) throws Exception {
    web.ignoring().antMatchers("/public/**");
  }
  protected void configure(HttpSecurity http) throws Exception {
    http.authorizeRequests().anyRequest().authenticated();
  }
}
"#;
    assert!(extract_spring_security_posture("C.java", src).is_none());
}

#[test]
fn a_minimal_secure_by_default_chain_parses_with_no_exceptions() {
    let src = r#"
public class C {
  protected void configure(HttpSecurity http) throws Exception {
    http.authorizeRequests().anyRequest().authenticated();
  }
}
"#;
    let posture = extract_spring_security_posture("C.java", src).expect("parses");
    assert!(posture.permit_all.is_empty(), "everything is authenticated");
}

#[test]
fn route_authentication_matches_the_be_spring_semantics() {
    let posture = extract_spring_security_posture("WebSecurityConfig.java", BE_SPRING).unwrap();
    // The measured FP: PUT /user matches no permitAll -> authenticated (exempt).
    assert!(
        posture.route_is_authenticated("PUT", "/user"),
        "PUT /user is the be-spring FP"
    );
    // The other mutating routes are authenticated too (only GET /articles/** is permitAll).
    assert!(posture.route_is_authenticated("POST", "/articles"));
    assert!(posture.route_is_authenticated("PUT", "/articles/{}"));
    assert!(posture.route_is_authenticated("DELETE", "/articles/{}/comments/{}"));
    // Genuinely-open routes are NOT authenticated (permitAll) -> stay flagged.
    assert!(
        !posture.route_is_authenticated("POST", "/users"),
        "POST /users is permitAll"
    );
    assert!(!posture.route_is_authenticated("POST", "/users/login"));
    assert!(
        !posture.route_is_authenticated("GET", "/articles/{}"),
        "GET /articles/** is permitAll"
    );
    assert!(
        !posture.route_is_authenticated("GET", "/profiles/{}"),
        "/profiles/** permitAll"
    );
    assert!(
        !posture.route_is_authenticated("OPTIONS", "/anything"),
        "OPTIONS any path permitAll"
    );
    // POST /articles/** is NOT permitAll (only GET is) -> authenticated.
    assert!(posture.route_is_authenticated("POST", "/articles/{}/favorite"));
}

#[test]
fn ant_double_star_matches_the_prefix_itself_and_deeper() {
    let posture = extract_spring_security_posture(
        "C.java",
        r#"
public class C { protected void configure(HttpSecurity http) throws Exception {
  http.authorizeRequests().antMatchers("/api/**").permitAll().anyRequest().authenticated();
} }
"#,
    )
    .unwrap();
    // `/api/**` opens `/api`, `/api/x`, `/api/x/y` — all NOT authenticated.
    assert!(!posture.route_is_authenticated("GET", "/api"));
    assert!(!posture.route_is_authenticated("POST", "/api/users"));
    assert!(!posture.route_is_authenticated("PUT", "/api/users/{}"));
    // but a sibling `/apixyz` (no segment boundary) IS authenticated.
    assert!(posture.route_is_authenticated("GET", "/apixyz"));
}

#[test]
fn a_path_variable_permitall_matcher_matches_the_normalized_route_param() {
    // `permitAll("/users/{id}")` must match the `{}`-normalized route `/users/{}` — otherwise the OPEN
    // route would be wrongly reported authenticated (a false-clear). The two mirror halves normalize path
    // vars differently (`{}` vs `{id}`); `seg_glob` reconciles them.
    let posture = extract_spring_security_posture(
        "C.java",
        r#"
public class C { protected void configure(HttpSecurity http) throws Exception {
  http.authorizeRequests()
      .requestMatchers("/public/{id}").permitAll()
      .anyRequest().authenticated();
} }
"#,
    )
    .unwrap();
    // The permitAll `{id}` matcher covers the `{}` route -> NOT authenticated -> stays flagged (safe).
    assert!(
        !posture.route_is_authenticated("POST", "/public/{}"),
        "open param route must not be exempt"
    );
    // A different path is still authenticated.
    assert!(posture.route_is_authenticated("POST", "/private/{}"));
}

#[test]
fn authorize_http_requests_entrypoint_is_also_recognized() {
    let src = r#"
public class C {
  protected void configure(HttpSecurity http) throws Exception {
    http.authorizeHttpRequests().requestMatchers("/health").permitAll().anyRequest().authenticated();
  }
}
"#;
    let posture = extract_spring_security_posture("C.java", src).expect("parses");
    assert_eq!(
        posture.permit_all,
        vec![SpringAntMatcher {
            method: None,
            patterns: vec!["/health".into()]
        }]
    );
}
