//! Coverage for `extract_spring_security_posture`: the be-spring `configure(HttpSecurity)` chain, the
//! secure-by-default gate, permitAll enumeration (with/without HttpMethod), and the parse-all-or-nothing
//! safety bails (unrecognized clause, non-literal path, no anyRequest, restriction terminal).

use super::{extract_spring_security_posture, SpringAntMatcher, SpringPostureBail};

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
    assert_eq!(
        extract_spring_security_posture("C.java", src)
            .unwrap_err()
            .name(),
        "not-secure-by-default"
    );
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
    assert_eq!(
        extract_spring_security_posture("C.java", src)
            .unwrap_err()
            .name(),
        "not-secure-by-default"
    );
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
    assert_eq!(
        extract_spring_security_posture("C.java", src)
            .unwrap_err()
            .name(),
        "unrecognized-clause"
    );
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
    assert_eq!(
        extract_spring_security_posture("C.java", src)
            .unwrap_err()
            .name(),
        "non-literal-matcher"
    );
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
    assert_eq!(
        extract_spring_security_posture("C.java", src)
            .unwrap_err()
            .name(),
        "multiple-chains"
    );
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
    assert_eq!(
        extract_spring_security_posture("C.java", src)
            .unwrap_err()
            .name(),
        "chain-scoper"
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
    assert_eq!(
        extract_spring_security_posture("C.java", src)
            .unwrap_err()
            .name(),
        "chain-scoper"
    );
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
    assert_eq!(
        extract_spring_security_posture("C.java", regex)
            .unwrap_err()
            .name(),
        "chain-scoper",
        "regexMatcher scope must bail"
    );
    let plural = r#"
public class C { protected void configure(HttpSecurity http) throws Exception {
  http.requestMatchers().antMatchers("/api/**").and().authorizeRequests().anyRequest().authenticated();
} }
"#;
    assert_eq!(
        extract_spring_security_posture("C.java", plural)
            .unwrap_err()
            .name(),
        "chain-scoper",
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
    assert_eq!(
        extract_spring_security_posture("C.java", src)
            .unwrap_err()
            .name(),
        "web-security-ignoring"
    );
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

// ---------------------------------------------------------------------------------------------
// The four widened spellings (and the ones deliberately still bailing). Each case below is one row
// of the measured bail matrix: `lambda-dsl`, `two-chains`, `.access(..)`, `non-literal matcher`.
// ---------------------------------------------------------------------------------------------

/// The bail id for a source that must NOT yield a posture — the whole point of naming bails is that a
/// test can pin WHICH one, so a widening that trades one bail for another is visible.
fn bail(src: &str) -> &'static str {
    extract_spring_security_posture("C.java", src)
        .expect_err("must bail")
        .name()
}

#[test]
fn the_spring_6_lambda_dsl_parses() {
    // The registry clauses live INSIDE the customizer lambda, chained off its parameter.
    let src = r#"
public class C {
  @Bean
  SecurityFilterChain filterChain(HttpSecurity http) throws Exception {
    http.authorizeHttpRequests(auth -> auth.requestMatchers("/thing/open").permitAll().anyRequest().authenticated());
    return http.build();
  }
}
"#;
    let posture = extract_spring_security_posture("C.java", src).expect("lambda DSL parses");
    assert_eq!(
        posture.permit_all,
        vec![SpringAntMatcher {
            method: None,
            patterns: vec!["/thing/open".into()]
        }]
    );
    assert!(posture.route_is_authenticated("POST", "/thing/create"));
    assert!(!posture.route_is_authenticated("POST", "/thing/open"));
}

#[test]
fn two_authorize_http_requests_customizers_on_one_chain_fold() {
    // Spring applies BOTH customizers to the SAME registry, so the matchers from the first and the
    // `anyRequest` terminal from the second are one posture — folding, not bailing.
    let src = r#"
public class C {
  @Bean
  SecurityFilterChain filterChain(HttpSecurity http) throws Exception {
    http.authorizeHttpRequests(r -> { r.requestMatchers("/thing/open").permitAll(); })
        .authorizeHttpRequests(r -> r.anyRequest().authenticated());
    return http.build();
  }
}
"#;
    let posture = extract_spring_security_posture("C.java", src).expect("two customizers fold");
    assert_eq!(
        posture.permit_all,
        vec![SpringAntMatcher {
            method: None,
            patterns: vec!["/thing/open".into()]
        }]
    );
    assert!(posture.route_is_authenticated("POST", "/thing/create"));
}

#[test]
fn a_lambda_block_body_with_several_registry_statements_parses() {
    // Each statement is its own clause GROUP: flattening them would desync the matcher/terminal pairing.
    let src = r#"
public class C {
  @Bean
  SecurityFilterChain filterChain(HttpSecurity http) throws Exception {
    http.authorizeHttpRequests(reg -> {
      reg.requestMatchers("/a").permitAll();
      reg.requestMatchers(HttpMethod.OPTIONS).permitAll();
      reg.requestMatchers("/admin/**").denyAll();
      reg.anyRequest().authenticated();
    });
    return http.build();
  }
}
"#;
    let posture = extract_spring_security_posture("C.java", src).expect("block body parses");
    assert_eq!(
        posture.permit_all,
        vec![
            SpringAntMatcher {
                method: None,
                patterns: vec!["/a".into()]
            },
            SpringAntMatcher {
                method: Some("OPTIONS".into()),
                patterns: vec![]
            },
        ],
        "denyAll is recognized but never opens a route"
    );
}

#[test]
fn any_request_access_with_the_authenticated_authorization_manager_parses() {
    // The ONE `.access(..)` argument whose identity proves an authenticated default.
    let src = r#"
public class C {
  @Bean
  SecurityFilterChain filterChain(HttpSecurity http) throws Exception {
    http.authorizeHttpRequests(r -> r.requestMatchers("/open").permitAll()
        .anyRequest().access(AuthenticatedAuthorizationManager.authenticated()));
    return http.build();
  }
}
"#;
    let posture = extract_spring_security_posture("C.java", src).expect("access(authenticated())");
    assert!(posture.route_is_authenticated("POST", "/thing"));
    assert!(!posture.route_is_authenticated("POST", "/open"));
}

#[test]
fn any_request_access_with_a_dynamic_manager_ternary_bails() {
    // mall's terminal. Reading the whole ternary as secure-by-default is an error in the GENEROUS
    // direction: the live arm is a bean that could GRANT, so routes it opens would be cleared.
    let src = r#"
public class C {
  @Bean
  SecurityFilterChain filterChain(HttpSecurity http) throws Exception {
    http.authorizeHttpRequests(r -> r.anyRequest()
        .access(dynamicAuthorizationManager==null? AuthenticatedAuthorizationManager.authenticated():dynamicAuthorizationManager));
    return http.build();
  }
}
"#;
    assert_eq!(bail(src), "any-request-access-not-provable");
}

#[test]
fn any_request_access_with_a_spel_string_bails() {
    // `.access("isAuthenticated()")` is the deprecated SpEL spelling; 0 occurrences in the corpus, and
    // deciding a SpEL expression is a language this parse does not read. Named bail, not a silent None.
    let src = r#"
public class C { protected void configure(HttpSecurity http) throws Exception {
  http.authorizeRequests().antMatchers("/x").permitAll().anyRequest().access("isAuthenticated()");
} }
"#;
    assert_eq!(bail(src), "any-request-access-not-provable");
}

#[test]
fn a_non_literal_matcher_in_a_lambda_loop_bails_naming_what_binds_it() {
    // mall's whitelist shape. The bail must name the MATCHER argument and the iterable that binds it —
    // that pair is the hook a property-resolution pass needs; a bail on the `for` itself would not be.
    let src = r#"
public class C {
  @Bean
  SecurityFilterChain filterChain(HttpSecurity http) throws Exception {
    http.authorizeHttpRequests(registry -> {
      for (String url : ignoreUrlsConfig.getUrls()) {
        registry.requestMatchers(url).permitAll();
      }
      registry.requestMatchers(HttpMethod.OPTIONS).permitAll();
    })
    .authorizeHttpRequests(registry -> registry.anyRequest().authenticated());
    return http.build();
  }
}
"#;
    let err = extract_spring_security_posture("C.java", src).expect_err("must bail");
    assert_eq!(err.name(), "non-literal-matcher");
    assert_eq!(
        err,
        SpringPostureBail::NonLiteralMatcher {
            arg: "url".into(),
            bound_by: Some("ignoreUrlsConfig.getUrls()".into()),
        }
    );
}

#[test]
fn a_lambda_body_statement_that_cannot_be_enumerated_bails() {
    // An `if` could hide a `permitAll` behind a condition this parse does not evaluate.
    let src = r#"
public class C { protected void configure(HttpSecurity http) throws Exception {
  http.authorizeHttpRequests(r -> {
    if (devMode) { r.requestMatchers("/debug/**").permitAll(); }
    r.anyRequest().authenticated();
  });
} }
"#;
    assert_eq!(bail(src), "lambda-body");
}

#[test]
fn a_registry_chain_not_rooted_at_the_lambda_parameter_bails() {
    let src = r#"
public class C { protected void configure(HttpSecurity http) throws Exception {
  http.authorizeHttpRequests(r -> { other.requestMatchers("/x").permitAll(); r.anyRequest().authenticated(); });
} }
"#;
    assert_eq!(bail(src), "lambda-body");
}

#[test]
fn a_non_lambda_customizer_argument_bails() {
    // `Customizer.withDefaults()` configures the registry somewhere this parse cannot follow.
    let src = r#"
public class C { protected void configure(HttpSecurity http) throws Exception {
  http.authorizeHttpRequests(Customizer.withDefaults());
} }
"#;
    assert_eq!(bail(src), "lambda-body");
}

#[test]
fn a_chain_that_mixes_the_fluent_and_lambda_spellings_bails() {
    let src = r#"
public class C { protected void configure(HttpSecurity http) throws Exception {
  http.authorizeHttpRequests(r -> r.requestMatchers("/a").permitAll())
      .authorizeHttpRequests().anyRequest().authenticated();
} }
"#;
    assert_eq!(bail(src), "mixed-dsl");
}

#[test]
fn a_scoper_after_a_lambda_entrypoint_still_bails() {
    // In lambda mode the chain's TAIL is plain HttpSecurity config and is skipped — but a `Matcher`
    // method there is still a chain-level scoper, and the posture is then path-local, not global.
    let src = r#"
public class C { protected void configure(HttpSecurity http) throws Exception {
  http.authorizeHttpRequests(r -> r.anyRequest().authenticated())
      .securityMatcher("/api/**");
} }
"#;
    assert_eq!(bail(src), "chain-scoper");
}

#[test]
fn a_lambda_config_ignores_the_plain_http_security_tail() {
    // `csrf`/`sessionManagement`/`addFilterBefore` after the entrypoint configure the filter chain, not
    // the authorization registry — they must not force a bail.
    let src = r#"
public class C {
  @Bean
  SecurityFilterChain filterChain(HttpSecurity http) throws Exception {
    http.authorizeHttpRequests(r -> r.requestMatchers("/open").permitAll().anyRequest().authenticated())
        .csrf(AbstractHttpConfigurer::disable)
        .sessionManagement(c -> c.sessionCreationPolicy(SessionCreationPolicy.STATELESS))
        .addFilterBefore(jwtFilter, UsernamePasswordAuthenticationFilter.class);
    return http.build();
  }
}
"#;
    let posture = extract_spring_security_posture("C.java", src).expect("tail is skipped");
    assert!(posture.route_is_authenticated("POST", "/thing"));
}

#[test]
fn the_be_spring_jwt_lambda_config_parses() {
    // Verbatim from `corpus/oss/be-spring-jwt/.../WebSecurityConfig.java` — the corpus tree whose count
    // must NOT move (all 3 mutating routes are already exempt by other means) while a posture IS now
    // extracted. Both halves are the shape test; this is the extraction half.
    let src = r#"
public class WebSecurityConfig {
  @Bean
  public SecurityFilterChain filterChain(HttpSecurity http) throws Exception {
    http.csrf(csrf -> csrf.disable());
    http.sessionManagement(sm -> sm.sessionCreationPolicy(SessionCreationPolicy.STATELESS));
    http.authorizeHttpRequests(auth -> auth
        .requestMatchers("/users/signin", "/users/signup").permitAll()
        .requestMatchers("/h2-console/**").permitAll()
        .requestMatchers("/v3/api-docs/**", "/swagger-ui/**", "/swagger-ui.html").permitAll()
        .anyRequest().authenticated());
    http.addFilterBefore(new JwtTokenFilter(jwtTokenProvider), UsernamePasswordAuthenticationFilter.class);
    return http.build();
  }
}
"#;
    let posture = extract_spring_security_posture("WebSecurityConfig.java", src)
        .expect("be-spring-jwt posture");
    assert_eq!(
        posture.permit_all,
        vec![
            SpringAntMatcher {
                method: None,
                patterns: vec!["/users/signin".into(), "/users/signup".into()]
            },
            SpringAntMatcher {
                method: None,
                patterns: vec!["/h2-console/**".into()]
            },
            SpringAntMatcher {
                method: None,
                patterns: vec![
                    "/v3/api-docs/**".into(),
                    "/swagger-ui/**".into(),
                    "/swagger-ui.html".into()
                ]
            },
        ]
    );
    // The tree's own mutating routes: authenticated under this posture (they are ALSO exempt by other
    // means, which is why the corpus count does not move).
    assert!(posture.route_is_authenticated("DELETE", "/users/{}"));
    assert!(!posture.route_is_authenticated("POST", "/users/signin"));
    assert!(!posture.route_is_authenticated("POST", "/users/signup"));
}

#[test]
fn two_lambda_chains_in_two_methods_still_bail() {
    // Folding is scoped to ONE builder chain. Two SEPARATE chains are two postures whose relative
    // scoping is unresolved — the pre-existing all-or-nothing bail must survive the widening.
    let src = r#"
public class C {
  @Bean SecurityFilterChain a(HttpSecurity http) throws Exception {
    http.authorizeHttpRequests(r -> r.anyRequest().authenticated());
    return http.build();
  }
  @Bean SecurityFilterChain b(HttpSecurity http) throws Exception {
    http.authorizeHttpRequests(r -> r.requestMatchers("/**").permitAll());
    return http.build();
  }
}
"#;
    assert_eq!(bail(src), "multiple-chains");
}

#[test]
fn a_lambda_config_without_any_request_bails() {
    // mall-demo's shape: `requestMatchers("/**").permitAll()` and nothing else — open by default.
    let src = r#"
public class C {
  @Bean SecurityFilterChain filterChain(HttpSecurity http) throws Exception {
    http.authorizeHttpRequests(registry -> registry.requestMatchers("/**").permitAll());
    return http.build();
  }
}
"#;
    assert_eq!(bail(src), "not-secure-by-default");
}

#[test]
fn a_file_with_no_authorization_chain_is_named_not_a_config() {
    assert_eq!(bail("public class C { void f() { g(); } }"), "not-a-config");
}

#[test]
fn a_configurer_permit_all_on_the_chain_spine_bails() {
    // `formLogin(..).permitAll()` / `logout().permitAll()` open paths the authorization REGISTRY never
    // lists, so `permit_all` cannot see them — the same reason `WebSecurity.ignoring(` bails. In lambda
    // mode the chain tail is otherwise skipped, so without this the posture would clear a route on one.
    let lambda = r#"
public class C { protected void configure(HttpSecurity http) throws Exception {
  http.authorizeHttpRequests(r -> r.anyRequest().authenticated())
      .formLogin(f -> f.loginPage("/login").permitAll());
} }
"#;
    assert_eq!(bail(lambda), "configurer-permit-all");
    let fluent = r#"
public class C { protected void configure(HttpSecurity http) throws Exception {
  http.logout().permitAll().and().authorizeRequests().anyRequest().authenticated();
} }
"#;
    assert_eq!(bail(fluent), "configurer-permit-all");
}

#[test]
fn a_configurer_permit_all_in_a_sibling_statement_bails() {
    // The SAME hazard as the test above, written as two statements instead of one chain — Spring reads
    // `http.a(); http.b();` exactly as `http.a().b()`. Measured before `siblings`: the one-chain form
    // bailed and this returned a posture with an EMPTY `permit_all`, clearing `POST /thing/open` — which
    // `PermitAllSupport` genuinely opens, `loginPage` being the login-PROCESSING url too.
    let lambda = r#"
public class C {
  @Bean SecurityFilterChain filterChain(HttpSecurity http) throws Exception {
    http.authorizeHttpRequests(r -> r.anyRequest().authenticated());
    http.formLogin(f -> f.loginPage("/thing/open").permitAll());
    return http.build();
  }
}
"#;
    assert_eq!(bail(lambda), "configurer-permit-all");
    // and the classic-fluent registry split from a sibling `logout().permitAll()` the same way
    let fluent = r#"
public class C {
  protected void configure(HttpSecurity http) throws Exception {
    http.authorizeRequests().antMatchers("/x").permitAll().anyRequest().authenticated();
    http.logout().permitAll();
  }
}
"#;
    assert_eq!(bail(fluent), "configurer-permit-all");
}

#[test]
fn a_security_matcher_in_a_sibling_statement_bails() {
    // `securityMatcher` scopes the WHOLE filter chain, so the posture is path-LOCAL wherever the call
    // sits. Split into its own statement it used to be invisible, and the `/admin/**`-local posture was
    // applied tree-wide — clearing mutating routes that chain never guards.
    let src = r#"
public class C {
  @Bean SecurityFilterChain filterChain(HttpSecurity http) throws Exception {
    http.securityMatcher("/admin/**");
    http.authorizeHttpRequests(r -> r.anyRequest().authenticated());
    return http.build();
  }
}
"#;
    assert_eq!(bail(src), "chain-scoper");
}

#[test]
fn a_sibling_statement_on_a_different_builder_is_left_alone() {
    // The relate-by-base-name half of `siblings`, in both directions. A `formLogin(..permitAll())` on
    // ANOTHER object is not this chain's configuration and must not bail — otherwise the scan would eat
    // legitimate configs — while an ALIAS of the same object cannot be shown to be another object at all.
    let other = r#"
public class C {
  @Bean SecurityFilterChain filterChain(HttpSecurity http) throws Exception {
    http.authorizeHttpRequests(r -> r.requestMatchers("/open").permitAll().anyRequest().authenticated());
    somethingElse.formLogin(f -> f.loginPage("/nope").permitAll());
    return http.build();
  }
}
"#;
    let posture =
        extract_spring_security_posture("C.java", other).expect("unrelated builder ignored");
    assert!(posture.route_is_authenticated("POST", "/thing"));
    assert!(!posture.route_is_authenticated("POST", "/open"));
    let aliased = r#"
public class C {
  @Bean SecurityFilterChain filterChain(HttpSecurity http) throws Exception {
    HttpSecurity h = http;
    http.authorizeHttpRequests(r -> r.anyRequest().authenticated());
    h.formLogin(f -> f.loginPage("/thing/open").permitAll());
    return http.build();
  }
}
"#;
    assert_eq!(bail(aliased), "sibling-scope");
}

#[test]
fn an_entrypoint_chain_with_no_nameable_receiver_bails() {
    // Sibling statements are related to the entrypoint by the name its chain is rooted at. With no
    // receiver at all there is no such name, so no statement in the method can be shown to be about a
    // DIFFERENT builder — and one of them may open a path. A bail keeps findings; a guess clears them.
    let src = r#"
public class C extends AbstractHttpConfigurer {
  void configure() {
    authorizeHttpRequests(r -> r.anyRequest().authenticated());
  }
}
"#;
    assert_eq!(bail(src), "sibling-scope");
}

#[test]
fn a_sibling_hazard_outside_the_entrypoints_own_block_still_bails() {
    // The scan's scope is the enclosing METHOD, not the entrypoint's nearest block. A conditional
    // registry statement puts the entrypoint one level in while the `securityMatcher` stays at method
    // level — a block-scoped walk would never see it and would apply an `/admin/**`-local posture
    // globally, which is exactly the false clear the scoper bail exists to prevent.
    let src = r#"
public class C {
  @Bean SecurityFilterChain filterChain(HttpSecurity http) throws Exception {
    http.securityMatcher("/admin/**");
    if (strict) {
      http.authorizeHttpRequests(r -> r.anyRequest().authenticated());
    }
    return http.build();
  }
}
"#;
    assert_eq!(bail(src), "chain-scoper");
}
