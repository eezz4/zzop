// code-hygiene/no-system-dialogs — bad: a blocking system dialog. good: a non-blocking custom modal.
declare function openModal(message: string): void;

export function bad() {
  alert('saved');
}

export function good() {
  openModal('saved');
}
