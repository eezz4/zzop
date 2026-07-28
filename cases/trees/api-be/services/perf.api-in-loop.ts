// perf/api-in-loop — bad: a network call per loop iteration. good: a single batched call.
declare function fetch(u: string): Promise<unknown>;

export async function bad(ids: string[]) {
  for (const id of ids) {
    await fetch('https://hook.example.com/notify?id=' + id);
  }
}

export function good(ids: string[]) {
  return fetch('https://hook.example.com/notify?ids=' + ids.join(','));
}
