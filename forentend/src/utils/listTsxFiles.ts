/**
 * Listing `.tsx` sources for the repo-wide accessibility and keyboard audits.
 *
 * ## Why this exists rather than `fs.globSync`
 *
 * Those audits used `globSync` from `node:fs`, which was added in Node 22. CI runs
 * Node 20 (`node-version: 20`), where it does not exist, so all three suites died
 * with `TypeError: globSync is not a function` while passing perfectly on the
 * Node 24 used for local development. The failure was invisible locally and total
 * in CI, which is the worst shape a test dependency can have: a check that only
 * runs where it cannot fail proves nothing.
 *
 * `readdirSync` with `withFileTypes` has been available since Node 10 and this
 * recursion uses nothing newer, so the same code runs on every version the
 * workflow might use. Nothing here consults `typescript` for the listing either,
 * so the set of files an audit covers does not depend on which TypeScript happens
 * to be installed -- which is the same class of dependency that made the original
 * version fragile.
 *
 * ## Paths
 *
 * Returns forward-slash paths relative to the working directory, which is what
 * `globSync` returned and what the callers print in their failure messages. On
 * Windows the separators are normalised, because a native backslash path would
 * not match the test-file patterns the callers match on.
 */

import { readdirSync, type Dirent } from 'node:fs';
import { join } from 'node:path';

/** Directories never worth walking into. `node_modules` is the one that matters:
 *  a repo-wide sweep would otherwise spend most of its time in it. */
const SKIP_DIRS = new Set(['node_modules', 'dist', 'build', 'coverage', '.git']);

function walk(dir: string, out: string[]): void {
  let entries: Dirent[];
  try {
    entries = readdirSync(dir, { withFileTypes: true });
  } catch {
    // A directory that cannot be read is not a reason to fail the audit; it simply
    // contributes no files. Without this, one unreadable path turns every remaining
    // check into an error instead of a result.
    return;
  }
  for (const entry of entries) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) {
      if (SKIP_DIRS.has(entry.name) || entry.name.startsWith('.')) continue;
      walk(full, out);
    } else if (entry.isFile()) {
      out.push(full.split('\\').join('/'));
    }
  }
}

/**
 * Every `.tsx` file under `root`, relative and forward-slashed.
 *
 * `exclude` is applied to each path after normalisation, so a caller passes the
 * same predicate it would previously have handed `globSync` as an exclusion
 * pattern.
 */
export function listTsxFiles(
  root: string,
  exclude: (relativePath: string) => boolean = () => false,
): string[] {
  const normalisedRoot = root.split('\\').join('/').replace(/\/+$/, '');
  const found: string[] = [];
  walk(normalisedRoot, found);
  return found.filter((p) => p.endsWith('.tsx') && !exclude(p)).sort();
}

/** The exclusion these audits have always used: a test file is not itself subject
 *  to the audit it performs, or every audit would report on the audits. */
export const isTestFile = (p: string): boolean => /\.test\.tsx$/.test(p);
