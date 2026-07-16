//! Coverage for `extract_controller_provides`: every prefix/path shape, the `@All` skip, the
//! non-controller-class gate, and the dynamic-argument (never-guess) skips.

use zzop_core::IoProvide;

mod body_shape;
mod guards;
mod provides;
mod provides_shapes;

fn keys(out: &[IoProvide]) -> Vec<String> {
    out.iter().map(|p| p.key.clone()).collect()
}
