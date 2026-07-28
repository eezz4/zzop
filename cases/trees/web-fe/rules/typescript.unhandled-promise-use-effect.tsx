// typescript/unhandled-promise-use-effect — bad: async useEffect callback (returned Promise dropped).
// good: sync callback that fires the async work via an inner void call.
import { useEffect } from 'react';

declare function load(): Promise<void>;

export function Bad() {
  useEffect(async () => {
    await load();
  }, []);
  return null;
}

export function Good() {
  useEffect(() => {
    void load();
  }, []);
  return null;
}
