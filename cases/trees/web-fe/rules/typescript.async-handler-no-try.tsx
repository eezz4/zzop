// typescript/async-handler-no-try — bad: an async JSX handler with an await but no try/catch.
// good: the same handler with a try/catch around the await.
declare function save(): Promise<void>;

export function Bad() {
  return <button onClick={async () => { await save(); }}>x</button>;
}

export function Good() {
  return <button onClick={async () => { try { await save(); } catch { /* handled */ } }}>x</button>;
}
