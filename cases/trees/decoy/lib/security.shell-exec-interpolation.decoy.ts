// DECOY for security/shell-exec-interpolation. In scope, provably: the rule's require_file
// (`child_process`) is satisfied by the comment below, so the file was scanned. Its two arms want
// `exec`/`execSync` called with an interpolated template or a concatenated string; `execFile` with an
// argv array and `execSync` with a fixed literal are the safe forms and match neither.
// thin wrapper over node child_process.
export declare function execFile(cmd: string, args: readonly string[]): Promise<string>;
export declare function execSync(cmd: string): string;

export function gitStatus(): Promise<string> {
  return execFile('git', ['status', '--porcelain']);
}

export function listFiles(): string {
  return execSync('ls -la');
}
