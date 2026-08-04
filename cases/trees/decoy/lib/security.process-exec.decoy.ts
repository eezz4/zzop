// DECOY — no rule may fire anywhere in this file (a finding here is a false positive by definition).
// The exact false-positive class W3's structural gate retires: this file MENTIONS child_process (so
// the rule's require_file pre-gate admits it) and every line below carries the interpolation shape
// the lexical arm looks for — but none of them is a projected `process-exec` site, because the callee
// resolves to something else in each case:
//   * `re.exec(...)` is RegExp's own method, on a receiver that is not a module binding;
//   * `execa` is a third-party runner the producer deliberately does not claim;
//   * the local helper named `exec` is this file's own function, not the import.
import { execa } from "execa";

// thin wrapper over node child_process, kept here so the pre-gate really does admit this file.
function exec(cmd: string): string {
  return cmd;
}

export function parseVersion(re: RegExp, out: string) {
  return re.exec(`version: ${out}`);
}

export function runThirdParty(name: string) {
  return execa(`tar -czf /tmp/${name}.tgz`);
}

export function runLocalHelper(name: string) {
  return exec(`tar -czf /tmp/${name}.tgz`);
}
