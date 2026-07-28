// be-security/path-traversal — bad: a request value is joined into a filesystem path. good: a fixed path
// (the safe pattern is to not let request input select the file at all).
import * as fs from 'fs';
import * as path from 'path';
interface Req { params: Record<string, string> }
const ROOT = '/srv/files';

export function bad(req: Req) {
  return fs.readFile(path.join(ROOT, req.params.name), () => {});
}

export function good(_req: Req) {
  return fs.readFile(path.join(ROOT, 'index.html'), () => {});
}
