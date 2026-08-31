//! Node-shape recognizers for the gin adapter — pure `Node`/`ImportMap` -> answer functions with
//! no collector state between them. Split from `gin.rs` for the file-size guard; the seam is that
//! nothing here decides anything about routes, it only reports what a node IS.

use std::collections::HashSet;

use tree_sitter::Node;
use zzop_core::ImportMap;

use crate::util::node_text;

pub(super) fn local_names(imports: &ImportMap) -> HashSet<String> {
    imports
        .iter()
        .filter(|(_, b)| b.specifier == "github.com/gin-gonic/gin")
        .map(|(local, _)| local.clone())
        .collect()
}

/// `<receiver>.<Method>(...)` -> `(receiver name, method name)`, `None` for any other call shape.
pub(super) fn selector_call<'s>(call: Node, src: &'s str) -> Option<(&'s str, &'s str)> {
    let func = call.child_by_field_name("function")?;
    if func.kind() != "selector_expression" {
        return None;
    }
    let operand = func.child_by_field_name("operand")?;
    let field = func.child_by_field_name("field")?;
    if operand.kind() != "identifier" {
        return None;
    }
    Some((node_text(operand, src), node_text(field, src)))
}
