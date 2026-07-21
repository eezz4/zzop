//! Coverage for `extract_spring_guarded_lines`: method-level and class-level Spring method-security
//! annotations, the route gate (a guarded non-route method is ignored), non-controller exclusion, and the
//! line contract (the emitted line equals the mapping-annotated method's `provides::extract` anchor line).

use super::extract_spring_guarded_lines;
use crate::provides::extract_http_provides;

#[test]
fn a_method_level_preauthorize_on_a_mutating_route_is_guarded() {
    // The be-spring-jwt shape: a DELETE handler guarded only by `@PreAuthorize` (invisible to the call
    // graph). `@DeleteMapping` is on the first modifier line, so that is the route's own anchor line.
    let src = "\
@RestController
@RequestMapping(\"/users\")
public class UserController {
  @DeleteMapping(value = \"/{username}\")
  @PreAuthorize(\"hasRole('ROLE_ADMIN')\")
  public String delete(@PathVariable String username) {
    return username;
  }
}
";
    let lines = extract_spring_guarded_lines("UserController.java", src);
    assert_eq!(
        lines,
        vec![4],
        "the @DeleteMapping method's first-modifier line"
    );
    // Line contract: the guarded line equals the route provide's own anchor line.
    let provides = extract_http_provides("UserController.java", src);
    assert_eq!(provides.len(), 1, "{provides:?}");
    assert_eq!(provides[0].key, "DELETE /users/{}");
    assert!(
        lines.contains(&provides[0].line),
        "guarded line matches the provide anchor"
    );
}

#[test]
fn a_class_level_annotation_guards_every_route_method() {
    let src = "\
@RestController
@RequestMapping(\"/admin\")
@Secured(\"ROLE_ADMIN\")
public class AdminController {
  @PostMapping(\"/ban\")
  public void ban() {}

  @DeleteMapping(\"/purge\")
  public void purge() {}
}
";
    let mut lines = extract_spring_guarded_lines("AdminController.java", src);
    lines.sort_unstable();
    assert_eq!(
        lines,
        vec![5, 8],
        "both route methods guarded by the class-level @Secured"
    );
}

#[test]
fn rolesallowed_and_postauthorize_are_recognized() {
    let src = "\
@RestController
public class C {
  @PostMapping(\"/a\")
  @RolesAllowed(\"ADMIN\")
  public void a() {}

  @PutMapping(\"/b\")
  @PostAuthorize(\"returnObject.owner == authentication.name\")
  public void b() {}
}
";
    let mut lines = extract_spring_guarded_lines("C.java", src);
    lines.sort_unstable();
    assert_eq!(lines, vec![3, 7]);
}

#[test]
fn a_guarded_method_that_is_not_a_route_is_ignored() {
    // `@PreAuthorize` on a plain (non-mapping) method mints no line — only registered routes matter.
    let src = "\
@RestController
public class C {
  @PreAuthorize(\"hasRole('ROLE_ADMIN')\")
  public void helper() {}

  @GetMapping(\"/x\")
  public void x() {}
}
";
    assert!(
        extract_spring_guarded_lines("C.java", src).is_empty(),
        "helper is not a route; x is not guarded"
    );
}

#[test]
fn an_unguarded_route_mints_no_line() {
    let src = "\
@RestController
public class C {
  @PostMapping(\"/open\")
  public void open() {}
}
";
    assert!(extract_spring_guarded_lines("C.java", src).is_empty());
}

#[test]
fn a_security_annotation_outside_a_controller_is_ignored() {
    // A `@Service` (non-controller) with `@PreAuthorize` has no routes to guard.
    let src = "\
@Service
public class UserService {
  @PreAuthorize(\"hasRole('ROLE_ADMIN')\")
  @PostMapping(\"/nope\")
  public void nope() {}
}
";
    assert!(extract_spring_guarded_lines("UserService.java", src).is_empty());
}

#[test]
fn a_nested_controller_gates_on_its_own_class_annotation() {
    // The outer class is @Secured; a nested controller without its own security annotation is NOT guarded
    // by the outer one (nested types gate independently, mirroring the provides pass).
    let src = "\
@Secured(\"ROLE_ADMIN\")
public class Outer {
  @RestController
  public static class Inner {
    @PostMapping(\"/x\")
    public void x() {}
  }
}
";
    assert!(
        extract_spring_guarded_lines("Outer.java", src).is_empty(),
        "the inner controller has no security annotation of its own"
    );
}

#[test]
fn a_guarded_non_literal_method_path_route_is_still_exempted() {
    // Regression (opus BLOCKING): the whole-corpus pass RESOLVES a non-literal method-path constant
    // (`@PostMapping(ApiPaths.CREATE)`) into a real route, so its guard-line MUST be emitted even though the
    // per-file `extract_http_provides` drops it (no corpus). Uses `method_route_states` (route MEMBERSHIP),
    // not the per-file `method_route` (which would drop this and desync the exemption -> a false
    // `mutating-route-no-auth` on an actually `@PreAuthorize`-guarded route).
    let src = "\
@RestController
@RequestMapping(\"/api\")
public class UserController {
  @PostMapping(ApiPaths.CREATE)
  @PreAuthorize(\"hasRole('ADMIN')\")
  public String create() { return \"\"; }
}
";
    let lines = extract_spring_guarded_lines("UserController.java", src);
    assert_eq!(
        lines,
        vec![4],
        "the guarded non-literal-path route's anchor line must be exempted"
    );
}
