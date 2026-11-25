// Runs every *-check.mjs under this folder, so `pnpm check` needs no list to keep up to date.
import { spawnSync } from 'node:child_process';
import { readdirSync } from 'node:fs';
import { join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('.', import.meta.url));
const checks = readdirSync(root, { recursive: true })
  .filter((name) => name.endsWith('-check.mjs'))
  .sort();
let failed = 0;
for (const name of checks) {
  const file = join(root, name);
  console.log(`\n${relative(root, file)}`);
  const run = spawnSync(process.execPath, ['--disable-warning=ExperimentalWarning', file], {
    stdio: 'inherit',
  });
  if (run.status !== 0) failed++;
}
console.log(`\n${checks.length - failed} of ${checks.length} checks passed.`);
process.exitCode = failed ? 1 : 0;
