// be-reliability/fetch-no-timeout — require_file wants a server signal, so this module imports express.
// bad: an outbound fetch with no timeout/AbortController. good: a fetch wired to an AbortController signal.
import express from 'express';
export const application = express();

declare function fetch(u: string, init?: unknown): Promise<{ json(): Promise<unknown> }>;

export async function bad() {
  const r = await fetch('https://svc.example.com/data');
  return r.json();
}

export async function good() {
  const controller = new AbortController();
  setTimeout(() => controller.abort(), 3000);
  const r = await fetch('https://svc.example.com/data', { signal: controller.signal });
  return r.json();
}
