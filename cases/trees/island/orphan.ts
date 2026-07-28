// Orphan leaf — imported ONLY by deadA (fanIn=1), not part of the cycle. Its sole importer is unreachable,
// so it is unreachable too: a clean single-rule `unreachable` node with no `circular` overlap.
export const orphan = 2;
