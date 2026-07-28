// fullstack/ws-no-auth — bad: a WebSocket opened with no auth material in the same function. good: the
// connection carries a token.
declare class WebSocket { constructor(url: string) }

export function bad() {
  const ws = new WebSocket('wss://rt.example.com/stream');
  return ws;
}

export function good(token: string) {
  const ws = new WebSocket('wss://rt.example.com/stream?token=' + token);
  return ws;
}
