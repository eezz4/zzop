// dead-candidates (file has no importers) + dead-exports (its export is imported nowhere). The barrel
// deliberately does NOT import this module, so both signals fire. Anchors: dead-candidates at line 1,
// dead-exports at the export's declaration line.
export const orphaned = 41;
