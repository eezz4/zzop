// be-reliability/interval-no-clear — bad: setInterval with no matching clearInterval anywhere in the file.
// FILE-level rule, so no `good` here (a clearInterval added for a good example would clear bad's flag);
// the fix is to keep the handle and clearInterval it on teardown.
declare function poll(): void;

export function bad() {
  setInterval(() => poll(), 1000);
}
