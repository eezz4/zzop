// browser/no-document-write — bad: document.write. good: a normal DOM insertion.
export function bad() {
  document.write('<p>hi</p>');
}

export function good() {
  document.body.append('hi');
}
