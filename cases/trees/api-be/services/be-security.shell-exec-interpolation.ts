// be-security/shell-exec-interpolation — bad: a template-literal interpolation spliced into the
// command string `exec` hands to a shell. good: `execFile` with an argv array, which no shell parses.
//
// Both halves of the rule's gate hold on the bad line and that is the point of this fixture after W3:
// the lexical arm sees the interpolation shape, and the parser projects a `process-exec` call site on
// the same line because `exec` here RESOLVES to this file's own `child_process` import. The decoy at
// decoy/lib/security.process-exec.decoy.ts is the control for the other direction — the same spelling
// on receivers that are not that binding.
import { exec, execFile } from "node:child_process";

export function badArchive(name: string) {
  exec(`tar -czf /tmp/${name}.tgz /var/data`);
}

export function good(name: string) {
  execFile("tar", ["-czf", `/tmp/${name}.tgz`, "/var/data"]);
}
