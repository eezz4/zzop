//! Coverage for `parse_exported_signature_names`. The three anchor cases are the measured shapes
//! from the `unimported-export` noise sample: the false positive this fact exists to kill, and the two
//! true positives that MUST survive it.
use super::parse_exported_signature_names;

fn names(src: &str) -> Vec<String> {
    parse_exported_signature_names("x.ts", src)
}

fn has(src: &str, name: &str) -> bool {
    names(src).iter().any(|n| n == name)
}

// -- The FALSE POSITIVE this module exists to eliminate --

#[test]
fn exported_function_return_type_is_public_signature() {
    let src = concat!(
        "export interface XState { count: number }\n",
        "export function useX(): XState {\n",
        "  return { count: 0 };\n",
        "}\n"
    );
    assert!(has(src, "XState"), "{:?}", names(src));
}

#[test]
fn exported_arrow_const_return_type_is_public_signature() {
    // The dominant React-hook shape: `export const useX = (): XState => {…}`.
    let src = concat!(
        "export interface XState { count: number }\n",
        "export const useX = (): XState => ({ count: 0 });\n"
    );
    assert!(has(src, "XState"), "{:?}", names(src));
}

#[test]
fn exported_param_and_wrapped_return_types_are_public_signature() {
    let src = concat!(
        "export async function save(input: SaveInput): Promise<SaveResult> {\n",
        "  return doIt(input);\n",
        "}\n"
    );
    let got = names(src);
    for want in ["SaveInput", "Promise", "SaveResult"] {
        assert!(got.iter().any(|n| n == want), "want {want} in {got:?}");
    }
}

// -- The TRUE POSITIVES that must keep firing --

#[test]
fn body_only_generic_is_not_a_public_signature() {
    // TP 1 from the sample: a type used only as an internal `useState<T>` generic, in a hook with
    // NO annotated return type. Nothing about it is public, so it must not be collected.
    let src = concat!(
        "export interface XState { count: number }\n",
        "export function useX() {\n",
        "  const [s, setS] = useState<XState>({ count: 0 });\n",
        "  return { s, setS };\n",
        "}\n"
    );
    assert!(!has(src, "XState"), "{:?}", names(src));
}

#[test]
fn type_annotating_an_unexported_declaration_is_not_public() {
    // TP 2 from the sample: the type only annotates a field of an UNEXPORTED `Props`.
    let src = concat!(
        "export interface XThing { id: string }\n",
        "interface Props { thing: XThing }\n",
        "function render(p: Props) { return p.thing.id; }\n"
    );
    assert!(!has(src, "XThing"), "{:?}", names(src));
}

#[test]
fn body_only_type_assertion_and_local_annotation_are_not_public() {
    let src = concat!(
        "export function useX() {\n",
        "  const local: XState = load() as XState;\n",
        "  return local;\n",
        "}\n"
    );
    assert!(!has(src, "XState"), "{:?}", names(src));
}

// -- Exported-ness gating --

#[test]
fn unexported_function_signature_contributes_nothing() {
    let src = "function useX(): XState { return null as any; }\n";
    assert!(!has(src, "XState"), "{:?}", names(src));
}

#[test]
fn bare_export_brace_publishes_a_local_declaration_signature() {
    let src = concat!(
        "function useX(): XState { return null as any; }\n",
        "export { useX };\n"
    );
    assert!(has(src, "XState"), "{:?}", names(src));
}

#[test]
fn re_export_from_another_module_does_not_publish_local_signatures() {
    // `export { useX } from "./y"` re-exports SOMEONE ELSE's declaration; the same-named local
    // function below is not thereby exported.
    let src = concat!(
        "function useX(): XState { return null as any; }\n",
        "export { useX } from \"./y\";\n"
    );
    assert!(!has(src, "XState"), "{:?}", names(src));
}

// -- Declaration kinds --

#[test]
fn exported_interface_body_and_extends_are_public_shape() {
    let src = concat!(
        "export interface Props extends BaseProps {\n",
        "  state: XState;\n",
        "  onChange(next: XState): void;\n",
        "}\n"
    );
    let got = names(src);
    for want in ["BaseProps", "XState"] {
        assert!(got.iter().any(|n| n == want), "want {want} in {got:?}");
    }
}

#[test]
fn exported_type_alias_right_hand_side_is_public() {
    let src = "export type Result = Ok | Err;\n";
    let got = names(src);
    for want in ["Ok", "Err"] {
        assert!(got.iter().any(|n| n == want), "want {want} in {got:?}");
    }
}

#[test]
fn exported_class_signature_but_not_method_bodies() {
    let src = concat!(
        "export class Store extends Base implements Syncable {\n",
        "  private state: XState;\n",
        "  constructor(cfg: StoreConfig) { super(); const hidden: Secret = mk(); }\n",
        "  save(input: SaveInput): SaveResult { const tmp: Internal = null as any; return tmp; }\n",
        "}\n"
    );
    let got = names(src);
    for want in [
        "Base",
        "Syncable",
        "XState",
        "StoreConfig",
        "SaveInput",
        "SaveResult",
    ] {
        assert!(got.iter().any(|n| n == want), "want {want} in {got:?}");
    }
    // Method/constructor BODY annotations stay private.
    for unwanted in ["Secret", "Internal"] {
        assert!(
            !got.iter().any(|n| n == unwanted),
            "{unwanted} must not leak from a body: {got:?}"
        );
    }
}

#[test]
fn export_default_function_signature_counts() {
    let src =
        "export default function handler(req: NextRequest): NextResponse { return null as any; }\n";
    let got = names(src);
    for want in ["NextRequest", "NextResponse"] {
        assert!(got.iter().any(|n| n == want), "want {want} in {got:?}");
    }
}

#[test]
fn generic_constraint_and_qualified_name_root() {
    let src = "export function pick<T extends Base>(x: NS.Inner<T>): T { return x as any; }\n";
    let got = names(src);
    for want in ["Base", "NS"] {
        assert!(got.iter().any(|n| n == want), "want {want} in {got:?}");
    }
    // `NS.Inner` contributes only its ROOT — `Inner` is a member of `NS`, not an importable symbol.
    assert!(!got.iter().any(|n| n == "Inner"), "{got:?}");
}

// -- Determinism / degrade --

#[test]
fn output_is_sorted_and_deduped() {
    let src = concat!(
        "export function a(x: Zed): Alpha { return null as any; }\n",
        "export function b(y: Zed): Alpha { return null as any; }\n"
    );
    let got = names(src);
    let mut sorted = got.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(got, sorted, "must be sorted and deduped: {got:?}");
}

#[test]
fn unparseable_file_degrades_to_empty() {
    assert!(names("export function ((( {").is_empty());
}
