// fullstack/get-with-body — bad: a GET request that carries a body. good: use POST for a body.
declare function request(o: unknown): Promise<unknown>;

export function bad() {
  return request({ method: 'get', body: JSON.stringify({ q: 1 }) });
}

export function good() {
  return request({ method: 'post', body: JSON.stringify({ q: 1 }) });
}
