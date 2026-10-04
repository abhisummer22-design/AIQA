/**
 * Tests for `listTsxFiles`, the replacement for `fs.globSync`.
 *
 * This helper exists because three repo-wide audits failed in CI with
 * `TypeError: globSync is not a function`: `fs.globSync` is Node 22+, the workflow
 * runs Node 20, and local development is on Node 24 where it exists -- so the
 * failure was invisible locally and total in CI.
 *
 * The tests below cannot prove Node 20 compatibility (they run on whatever Node is
 * running them), so that is asserted structurally instead: the helper imports
 * exactly two `node:` symbols, both far older than any supported runtime. What
 * *is* proven here is that the walk returns what the callers depend on, because a
 * helper that silently returns fewer files would turn three accessibility audits
 * into three vacuous ones -- which is the failure mode that matters most for a
 * check whose whole job is to notice what is there.
 */

import { existsSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { isTestFile, listTsxFiles } from './listTsxFiles';

describe('listTsxFiles', () => {
  it('finds the source files the audits are supposed to be reading', () => {
    const files = listTsxFiles('src/components');
    expect(files.length).toBeGreaterThan(100);
    // Named files that must be in any correct listing, across several directories.
    expect(files).toContain('src/components/pages/RBACTestingPage.tsx');
    expect(files).toContain('src/components/layout/Header.tsx');
    // Recursion has to descend, not just read one level.
    expect(files.some((f) => f.split('/').length > 3)).toBe(true);
  });

  it('returns only .tsx, so the audits never parse a .ts file as JSX', () => {
    const files = listTsxFiles('src');
    expect(files.every((f) => f.endsWith('.tsx'))).toBe(true);
    expect(files.some((f) => f.endsWith('.ts') && !f.endsWith('.d.ts'))).toBe(false);
  });

  it('excludes test files, which is what keeps an audit from auditing itself', () => {
    const withTests = listTsxFiles('src/components');
    const withoutTests = listTsxFiles('src/components', isTestFile);
    expect(withTests.length).toBeGreaterThan(withoutTests.length);
    expect(withoutTests.some((f) => isTestFile(f))).toBe(false);
    // And a real `.tsx` audit file is in the first list but not the second --
    // otherwise every audit would report on the audits. (This test file is a `.ts`,
    // so it is in neither list; the helper only ever returns `.tsx`.)
    expect(withTests.some((f) => f.endsWith('a11ySemantics.test.tsx'))).toBe(true);
    expect(withoutTests.some((f) => f.endsWith('a11ySemantics.test.tsx'))).toBe(false);
  });

  it('returns forward-slash relative paths on every platform', () => {
    for (const f of listTsxFiles('src/components')) {
      expect(f).not.toContain('\\');
      expect(existsSync(f)).toBe(true);
      expect(f.startsWith('src/')).toBe(true);
    }
  });

  it('is deterministic, so a failure names the same file twice', () => {
    const a = listTsxFiles('src/components');
    const b = listTsxFiles('src/components');
    expect(a).toEqual(b);
    expect(a).toEqual([...a].sort());
  });

  it('returns nothing for a directory that does not exist, rather than throwing', () => {
    expect(listTsxFiles('src/there-is-no-such-directory')).toEqual([]);
  });

  it('never descends into node_modules', () => {
    // The sweep is repo-wide, so an unfiltered walk would spend most of its time
    // and produce tens of thousands of unrelated files.
    const files = listTsxFiles('.');
    expect(files.some((f) => f.includes('node_modules'))).toBe(false);
  });
});

describe('isTestFile', () => {
  it('matches only .test.tsx, and not a component whose name merely contains test', () => {
    expect(isTestFile('src/a/b.test.tsx')).toBe(true);
    expect(isTestFile('src/a/TestThing.tsx')).toBe(false);
    expect(isTestFile('src/a/latest.tsx')).toBe(false);
    expect(isTestFile('src/a/b.tsx')).toBe(false);
  });
});