/**
 * Phase 8D — hand a text to the browser as a file to save (a JUnit report for a CI system). Nothing is sent anywhere: the text is made
 * into a file in memory, and the browser is asked to save it, as the Evidence page does with an artifact.
 */
export function saveTextFile(fileName: string, text: string, type = 'application/xml'): void {
  const url = URL.createObjectURL(new Blob([text], { type: `${type};charset=utf-8` }));
  const link = document.createElement('a');
  link.href = url;
  link.download = fileName;
  document.body.appendChild(link);
  link.click();
  link.remove();
  URL.revokeObjectURL(url);
}

/** A name for a file that is safe on every system: the letters, digits, dots, dashes and underscores of `text`, the rest a dash. */
export function fileNameOf(text: string, extension: string): string {
  const stem = text
    .trim()
    .replace(/[^A-Za-z0-9._-]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 60);
  return `${stem || 'report'}.${extension}`;
}
