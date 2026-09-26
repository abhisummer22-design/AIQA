/**
 * Phase 8D — handing a text to the browser as a file to save. jsdom has no `URL.createObjectURL`, so it is stubbed, and the click that
 * would start the save is watched instead of happening.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { fileNameOf, saveTextFile } from './saveTextFile';

type UrlApi = { createObjectURL: (b: Blob) => string; revokeObjectURL: (u: string) => void };
const url = URL as unknown as UrlApi;

const readBlob = (blob: Blob) => new Promise<string>((resolve, reject) => { const reader = new FileReader(); reader.onload = () => resolve(String(reader.result)); reader.onerror = () => reject(reader.error); reader.readAsText(blob); });

let made: Blob[] = [];
let revoked: string[] = [];
let clicked: Array<{ href: string; download: string; inDocument: boolean }> = [];

beforeEach(() => {
  made = [];
  revoked = [];
  clicked = [];
  url.createObjectURL = vi.fn((blob: Blob) => {
    made.push(blob);
    return 'blob:junit-test';
  });
  url.revokeObjectURL = vi.fn((u: string) => void revoked.push(u));
  vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(function (this: HTMLAnchorElement) {
    clicked.push({ href: this.href, download: this.download, inDocument: document.body.contains(this) });
  });
});

afterEach(() => vi.restoreAllMocks());

describe('saveTextFile', () => {
  it('makes a file of the text, asks the browser to save it under the name, and leaves nothing behind', async () => {
    saveTextFile('regression-run.xml', '<testsuites/>');
    expect(made).toHaveLength(1);
    expect(made[0].type).toBe('application/xml;charset=utf-8');
    expect(await readBlob(made[0])).toBe('<testsuites/>');
    expect(clicked).toEqual([{ href: 'blob:junit-test', download: 'regression-run.xml', inDocument: true }]);
    expect(revoked).toEqual(['blob:junit-test']);
    expect(document.querySelector('a[download]'), 'the link that started the save is taken away').toBeNull();
  });

  it('says what kind of text it is when it is not XML', () => {
    saveTextFile('notes.txt', 'hello', 'text/plain');
    expect(made[0].type).toBe('text/plain;charset=utf-8');
  });
});

describe('fileNameOf', () => {
  it('keeps letters, digits, dots, dashes and underscores, and makes a dash of anything else', () => {
    expect(fileNameOf('Nightly regression', 'xml')).toBe('Nightly-regression.xml');
    expect(fileNameOf('  Pets API v1.2 / staging  ', 'xml')).toBe('Pets-API-v1.2-staging.xml');
    expect(fileNameOf('a_b-c.d', 'xml')).toBe('a_b-c.d.xml');
  });

  it('is never empty, and never long', () => {
    expect(fileNameOf('', 'xml')).toBe('report.xml');
    expect(fileNameOf('///', 'xml')).toBe('report.xml');
    expect(fileNameOf('x'.repeat(200), 'xml')).toBe(`${'x'.repeat(60)}.xml`);
  });
});
