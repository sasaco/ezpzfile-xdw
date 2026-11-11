// The editor's page (media/editor.html, made by scripts/assemble.mjs) for one webview: its CSP, where its script and
// the engine's .wasm are, and its language.

/** What the upstream editor fetches from outside, kept as it is: its font's stylesheet, and pdf.js when a PDF is inserted. */
const FONT_CSS = 'https://cdn.jsdelivr.net/gh/orioncactus/pretendard@v1.3.9/';
const PDFJS = 'https://cdnjs.cloudflare.com/ajax/libs/pdf.js/3.11.174/';

export function webviewCsp(cspSource: string): string {
  return [
    "default-src 'none'",
    // 'wasm-unsafe-eval': the engine. blob: the pdf.js worker, which the editor starts from a local copy.
    `script-src ${cspSource} 'wasm-unsafe-eval' blob: ${PDFJS}`,
    `style-src ${cspSource} 'unsafe-inline' ${FONT_CSS}`,
    `img-src ${cspSource} data: blob:`,
    `font-src ${cspSource} data: ${FONT_CSS}`,
    `connect-src ${cspSource} data: blob: ${PDFJS}`,
    // The Worker that checks a file before the editor opens it, and pdf.js's.
    'worker-src blob:',
    "frame-src 'none'",
    "object-src 'none'",
    "base-uri 'none'",
  ].join('; ');
}

export type PageParts = { cspSource: string; scriptUri: string; wasmUri: string; language: string };

export function editorHtml(template: string, parts: PageParts): string {
  const fill = (text: string, key: string, value: string): string => {
    const mark = `{{${key}}}`;
    if (!text.includes(mark)) throw new Error(`editor.html: ${mark} is missing`);
    return text.replace(mark, () => escapeAttribute(value));
  };
  let page = fill(template, 'csp', webviewCsp(parts.cspSource));
  page = fill(page, 'wasm', parts.wasmUri);
  page = fill(page, 'script', parts.scriptUri);
  // The editor has Japanese and English (i18n.js reads <html lang>).
  const lang = parts.language.toLowerCase().startsWith('ja') ? 'ja' : 'en';
  if (!page.includes('<html lang="ja">')) throw new Error('editor.html: <html lang="ja"> is missing');
  return page.replace('<html lang="ja">', `<html lang="${lang}">`);
}

function escapeAttribute(text: string): string {
  return text.replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c] ?? c);
}
