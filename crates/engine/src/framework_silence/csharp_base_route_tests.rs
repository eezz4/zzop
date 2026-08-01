//! S13's tests. Split out of `csharp_base_route.rs` to keep that file under the line cap.
//!
//! Every test under "the first cut's defects" below is a headstone: the first version of this module
//! shipped each of those behaviours and a 3-stage review reproduced every one against real fixtures.
//! They are pinned individually so a future simplification of the lexical scan cannot quietly restore one.

use super::scan::is_interface_name;
use super::*;
use crate::framework_silence::tests::TempDir;

/// One `.cs` file's warning, or `None`.
fn warn(name: &str, body: &str) -> Option<String> {
    let dir = TempDir::new("zzop-cs-base");
    dir.write(name, body);
    csharp_base_route_warning(dir.path(), &[name.to_string()])
}

#[test]
fn a_project_base_without_own_route_is_reported() {
    let w = warn(
        "UsersController.cs",
        "public class UsersController : ApiBaseController\n{\n    [HttpGet(\"users\")]\n    public void List() {}\n}\n",
    )
    .expect("warning");
    assert!(w.contains("UsersController : ApiBaseController"), "{w}");
    assert!(w.contains("wrong key"), "names the failure mode: {w}");
}

/// EVERY ASP.NET controller derives from `ControllerBase`. A warning that fires on all of them names no
/// hazard and trains the reader to skip the line.
#[test]
fn the_framework_base_is_not_a_hazard() {
    assert!(warn(
        "UsersController.cs",
        "public class UsersController : ControllerBase\n{\n    [HttpGet(\"users\")]\n    public void List() {}\n}\n",
    )
    .is_none());
}

/// A class carrying its own `[Route]` has nothing inherited that could move its key.
#[test]
fn a_class_with_its_own_route_is_silent() {
    assert!(warn(
        "UsersController.cs",
        "[Route(\"api/v1/[controller]\")]\npublic class UsersController : ApiBaseController\n{\n    [HttpGet]\n    public void List() {}\n}\n",
    )
    .is_none());
}

#[test]
fn a_file_with_no_controller_surface_is_silent() {
    assert!(warn("Model.cs", "public class User : EntityBase\n{\n}\n").is_none());
}

// ---------------------------------------------------------------------------------------------
// The first cut's defects, each pinned dead.
// ---------------------------------------------------------------------------------------------

/// The one that mattered most: on `corpus/oss/be-aspnet`, 8 of 8 controllers are spelled this way, so
/// the first cut — which required the name token to be bare alphanumerics — emitted nothing at all on
/// the only real ASP.NET corpus this repo holds. A tripwire that cannot fire on the corpus it was
/// written for is not a conservative tripwire, it is an absent one.
#[test]
fn a_csharp_12_primary_constructor_is_read() {
    let w = warn(
        "UsersController.cs",
        "public class UsersController(IMediator mediator) : ApiBaseController\n{\n    [HttpGet(\"users\")]\n    public void List() {}\n}\n",
    )
    .expect("primary-constructor controllers must be visible");
    assert!(w.contains("UsersController : ApiBaseController"), "{w}");
}

/// A base list wrapped onto its own line is ordinary formatting, not an exotic spelling.
#[test]
fn a_wrapped_base_list_is_read() {
    let w = warn(
        "UsersController.cs",
        "public class UsersController\n    : ApiBaseController\n{\n    [HttpGet(\"users\")]\n    public void List() {}\n}\n",
    )
    .expect("wrapped base list must be visible");
    assert!(w.contains("UsersController : ApiBaseController"), "{w}");
}

/// The first cut counted every `class X : Y` in a `[Http`-bearing file, so an ordinary controller's own
/// nested request DTO produced a warning whose every word was false.
#[test]
fn a_nested_dto_inside_a_controller_is_not_a_controller() {
    assert!(warn(
        "UsersController.cs",
        "public class UsersController : ControllerBase\n{\n    [HttpPost(\"users\")]\n    public void Create() {}\n\n    public class Request : BaseRequest {}\n}\n",
    )
    .is_none());
}

/// Same gate, sibling shape: a handler or DTO declared beside the controller.
#[test]
fn a_sibling_handler_is_not_a_controller() {
    assert!(warn(
        "UsersController.cs",
        "public class UsersController : ControllerBase\n{\n    [HttpGet(\"users\")]\n    public void List() {}\n}\n\npublic class CreateUserCommand : IRequest\n{\n}\n",
    )
    .is_none());
}

/// The extractor's gate is `[ApiController]`/`[Controller]` OR a `Controller` name suffix — a class that
/// qualifies only by attribute must qualify here too, or S13 goes dark where the extractor keys routes.
#[test]
fn the_attribute_alone_makes_it_a_controller() {
    let w = warn(
        "Users.cs",
        "[ApiController]\npublic class UsersEndpoint : ApiBaseController\n{\n    [HttpGet(\"users\")]\n    public void List() {}\n}\n",
    )
    .expect("attribute-gated controller must be visible");
    assert!(w.contains("UsersEndpoint : ApiBaseController"), "{w}");
}

/// A deleted controller left behind as a comment is not serving anything.
#[test]
fn a_commented_out_class_is_not_a_controller() {
    assert!(warn(
        "UsersController.cs",
        "public class UsersController : ControllerBase\n{\n    [HttpGet(\"users\")]\n    public void List() {}\n}\n\n// public class OldUsersController : LegacyApiController\n// {\n//     [HttpGet] public void L() {}\n// }\n",
    )
    .is_none());
}

/// The first cut read the pair `Users : kept` out of this sentence and named it as a controller class.
#[test]
fn prose_in_a_doc_comment_names_no_class() {
    assert!(warn(
        "UsersController.cs",
        "/// <summary>Wraps the old class Users: kept for back-compat.</summary>\npublic class UsersController : ControllerBase\n{\n    [HttpGet(\"users\")]\n    public void List() {}\n}\n",
    )
    .is_none());
}

/// A class name inside a string literal is not a declaration either.
#[test]
fn a_class_name_in_a_string_literal_names_no_class() {
    assert!(warn(
        "UsersController.cs",
        "public class UsersController : ControllerBase\n{\n    [HttpGet(\"users\")]\n    public void List() { var s = \"public class Ghost : GhostBase\"; }\n}\n",
    )
    .is_none());
}

/// C# allows the attribute on the declaration line. The first cut walked strictly ABOVE the class line
/// and so never saw it — reporting a class that declares its own prefix.
#[test]
fn a_same_line_route_attribute_is_seen() {
    assert!(warn(
        "UsersController.cs",
        "[Route(\"api/users\")] public class UsersController : ApiBaseController\n{\n    [HttpGet]\n    public void List() {}\n}\n",
    )
    .is_none());
}

/// The worst direction: the first cut re-found each class by `contains(\"class {name}\")`, so an abstract
/// `UsersControllerBase` declared ABOVE `UsersController` matched first, and its `[Route]` silenced the
/// real suspect — a false negative of exactly the case this module exists to catch.
#[test]
fn a_name_prefix_collision_does_not_silence_the_real_suspect() {
    let w = warn(
        "UsersController.cs",
        "[Route(\"api/base\")]\npublic abstract class UsersControllerBase : ApiBaseController\n{\n}\n\npublic class UsersController : ApiBaseController\n{\n    [HttpGet(\"users\")]\n    public void List() {}\n}\n",
    )
    .expect("the suspect must survive a longer sibling name");
    assert!(w.contains("UsersController : ApiBaseController"), "{w}");
}

/// C# only requires the base CLASS to sit first IF there is one. An interface-only list means no base
/// class, hence no inherited prefix — the first cut reported the interface as a "PROJECT base class".
#[test]
fn an_interface_only_base_list_is_not_a_base_class() {
    assert!(warn(
        "UsersController.cs",
        "public class UsersController : IUsersController\n{\n    [HttpGet(\"users\")]\n    public void List() {}\n}\n",
    )
    .is_none());
}

/// …but a base class FOLLOWED by interfaces is still a base class.
#[test]
fn a_base_class_before_interfaces_is_read() {
    let w = warn(
        "UsersController.cs",
        "public class UsersController : ApiBaseController, IUsersController\n{\n    [HttpGet(\"users\")]\n    public void List() {}\n}\n",
    )
    .expect("base class before interfaces must be read");
    assert!(w.contains("UsersController : ApiBaseController"), "{w}");
}

/// A `where` constraint's `:` is not a base list.
#[test]
fn a_generic_constraint_colon_is_not_a_base_list() {
    assert!(warn(
        "UsersController.cs",
        "public class UsersController<T> where T : IEntity\n{\n    [HttpGet(\"users\")]\n    public void List() {}\n}\n",
    )
    .is_none());
}

/// One `partial` controller is one hazard, however many halves it is written in.
#[test]
fn partial_halves_count_once() {
    let dir = TempDir::new("zzop-cs-partial");
    dir.write(
        "UsersController.A.cs",
        "public partial class UsersController : ApiBaseController\n{\n    [HttpGet(\"users\")]\n    public void List() {}\n}\n",
    );
    dir.write(
        "UsersController.B.cs",
        "public partial class UsersController : ApiBaseController\n{\n    [HttpPost(\"users\")]\n    public void Create() {}\n}\n",
    );
    let w = csharp_base_route_warning(
        dir.path(),
        &[
            "UsersController.A.cs".to_string(),
            "UsersController.B.cs".to_string(),
        ],
    )
    .expect("warning");
    assert!(w.contains("1 controller class(es)"), "counted twice: {w}");
}

/// `IsAlphanumeric`-style names beginning with `I` are types, not interfaces.
#[test]
fn interface_detection_needs_the_pascal_shape() {
    assert!(!is_interface_name("Item"));
    assert!(!is_interface_name("IO"));
    assert!(is_interface_name("IUsersController"));
    assert!(is_interface_name("IRequest"));
}
