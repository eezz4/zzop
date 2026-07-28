// DECOY for security/eval-dynamic-code. In scope, and provably so: the rule's require_file
// (`\beval\s*\(|\bnew\s+Function\s*\(`) is satisfied twice below, so the file was scanned. Its two arms
// then need a NON-literal first argument (`eval-nonliteral`) or a non-empty one (`new-function`), and
// neither shape appears here.
export function constantFold(): unknown {
  // a string literal argument — the arm requires the first character after `(` to be something else.
  return eval('2 + 2');
}

export function emptyFunction(): unknown {
  // `new Function()` with no arguments — the arm requires at least one non-`)` character.
  return new Function();
}
