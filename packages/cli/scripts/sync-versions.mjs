// Syncs this package's version into every npm/<platform>/package.json and this package's own
// optionalDependencies pins, so a version bump only has to happen in one place
// (packages/cli/package.json) instead of six. Plain Node, no dependencies.
//
// Usage: node scripts/sync-versions.mjs

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const cliDir = path.join(__dirname, '..');
const npmDir = path.join(cliDir, 'npm');

function readJson(file) {
  return JSON.parse(fs.readFileSync(file, 'utf8'));
}

function writeJson(file, data) {
  fs.writeFileSync(file, `${JSON.stringify(data, null, 2)}\n`);
}

function main() {
  const rootPkgPath = path.join(cliDir, 'package.json');
  const rootPkg = readJson(rootPkgPath);
  const version = rootPkg.version;
  if (!version) {
    throw new Error(`sync-versions: ${rootPkgPath} has no "version" field`);
  }

  const platformDirs = fs
    .readdirSync(npmDir, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => entry.name)
    .sort();

  if (platformDirs.length === 0) {
    throw new Error(`sync-versions: no platform sub-packages found under ${npmDir}`);
  }

  let changed = 0;
  rootPkg.optionalDependencies ??= {};

  for (const dir of platformDirs) {
    const pkgPath = path.join(npmDir, dir, 'package.json');
    const pkg = readJson(pkgPath);

    if (pkg.version !== version) {
      pkg.version = version;
      writeJson(pkgPath, pkg);
      changed += 1;
      console.log(`sync-versions: ${path.relative(cliDir, pkgPath)} -> ${version}`);
    }

    if (rootPkg.optionalDependencies[pkg.name] !== version) {
      rootPkg.optionalDependencies[pkg.name] = version;
      changed += 1;
      console.log(`sync-versions: optionalDependencies["${pkg.name}"] -> ${version}`);
    }
  }

  if (changed > 0) {
    writeJson(rootPkgPath, rootPkg);
  }

  console.log(
    `sync-versions: ${platformDirs.length} platform package(s) at version ${version} ` +
      `(${changed} field(s) updated).`
  );
}

main();
