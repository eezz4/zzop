// DECOY for browser/no-system-dialogs. In scope, provably: the rule's require_file
// (`\b(?:alert|confirm|prompt)\s*\(`) is satisfied, AND its line_pattern matches the interface member
// below — only the `exclude_pattern` for a bare method-signature line keeps it silent. The call site is
// then a `.`-qualified custom dialog service, which the pattern's leading `[^.\w]` excludes.
export interface DialogService {
  confirm(message: string): Promise<boolean>;
  alert(message: string): Promise<void>;
}

export declare const dialogs: DialogService;

export function confirmDelete(): Promise<boolean> {
  return dialogs.confirm('Delete this entry?');
}
