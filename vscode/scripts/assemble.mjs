// Puts the upstream editor (../web: editor.html, editor/i18n.js, editor/app.js and the wasm-bindgen
// glue) together for a VS Code webview, as web/build.sh does for a web page. The differences:
//   - the module script is a file of its own (a webview's CSP then needs no inline script),
//   - the .wasm is loaded from the URL in <meta name="ezpzxdw-wasm"> (the extension fills it in),
//   - webview/host.js stands in for editor/host.js, and webview/tail.js follows app.js.
//   - the editor's check of a file before it opens it draws pages for CHECK_MS, not for eight seconds, and
//     does not refuse the file for a page the engine cannot draw (CHECK_DRAW).
// This changes no upstream file (the fork's one change to them is in web/editor/app.js: thumbnails are
// drawn only near what shows; ../README.md). Pure functions on text, so tests run them without a build.

import { createHash } from 'node:crypto';

const SCRIPT = /<script type="module">\n([\s\S]*?)<\/script>/;

/**
 * How long the editor draws pages in a Worker before it opens a file (upstream: 8000). The editor waits for it, so
 * with upstream's value every document longer than eight seconds of drawing shows eight seconds late. One second
 * still opens the file in the Worker (not a DocuWorks file, damaged, protected, too large) and draws its first
 * pages there; pages further on are not tried before the document shows, as upstream does not try those its eight
 * seconds do not reach.
 */
export const CHECK_MS = 1000;
const UPSTREAM_CHECK = 'const CHECK_MS = 8000;';

/**
 * What the check does with a page it cannot draw. Upstream refuses the whole file for one such page. The engine
 * refuses a page that is past its own limits by returning an error, and goes on working: the editor then says that
 * the page cannot be shown and shows the others, so the check lets such a page pass. A trap is another matter (the
 * Worker's engine is gone, as the editor's would be): the check fails for it as before.
 */
const UPSTREAM_DRAW = '{ doc.render(drawn); doc.forget(drawn); }';
export const CHECK_DRAW = '{ try { doc.render(drawn); } catch (err) { if (err instanceof WebAssembly.RuntimeError) throw err; } doc.forget(drawn); }';

/**
 * Text with the line endings git stores. A checkout on Windows with core.autocrlf has CRLF in every file, and what
 * is put together here (and the tests on it) is written for LF.
 */
export function lf(text) {
  return text.replace(/\r\n/g, '\n');
}

/** The glue without its exports, as build.sh strips them: its names become the script's own. */
export function stripExports(glue) {
  return lf(glue).replace(/^export \{[^}]*\};?\s*$/gm, '').replace(/^export (class|function|async function|const|let) /gm, '$1 ');
}

function replaceOnce(text, find, value, what) {
  const at = text.indexOf(find);
  if (at < 0 || text.indexOf(find, at + find.length) >= 0) throw new Error(`expected ${what} exactly once`);
  return text.slice(0, at) + value + text.slice(at + find.length);
}

/** The editor's module script. */
export function editorScript({ editorHtml, glue, i18n, host, app, tail }) {
  const found = SCRIPT.exec(lf(editorHtml));
  if (!found) throw new Error('editor.html: no <script type="module">');
  const stripped = stripExports(glue);
  let checked = replaceOnce(lf(app), UPSTREAM_CHECK, `const CHECK_MS = ${CHECK_MS};`, `app.js: ${UPSTREAM_CHECK}`);
  checked = replaceOnce(checked, UPSTREAM_DRAW, CHECK_DRAW, `app.js: ${UPSTREAM_DRAW}`);
  let script = found[1];
  script = replaceOnce(script, '/*__EZPZXDW_GLUE__*/', stripped, 'editor.html: the glue placeholder');
  script = replaceOnce(script, '"/*__EZPZXDW_WASM_URL__*/"', 'document.querySelector(\'meta[name="ezpzxdw-wasm"]\').content', 'editor.html: the .wasm URL placeholder');
  script = replaceOnce(script, '/*__EZPZXDW_GLUE_STR__*/""', JSON.stringify(stripped), 'editor.html: the glue string placeholder');
  script = replaceOnce(script, '/*__EZPZXDW_APP__*/', [lf(i18n), lf(host), checked, lf(tail)].join('\n'), 'editor.html: the app placeholder');
  return script;
}

/** The inline handlers of the page (onfocus="…"), each as the CSP hash that lets exactly that text run. */
export function handlerHashes(html) {
  const hashes = [...html.matchAll(/\son[a-z]+="([^"]*)"/g)].map((m) => `'sha256-${createHash('sha256').update(m[1]).digest('base64')}'`);
  return [...new Set(hashes)];
}

/**
 * The page: editor.html with the module script taken out. The extension fills in {{…}} (src/html.ts); the CSP it
 * writes is followed here by the hashes of the page's own inline handlers, so those run and no other inline script does.
 */
export function editorPage(editorHtml) {
  const html = lf(editorHtml);
  if (!SCRIPT.test(html)) throw new Error('editor.html: no <script type="module">');
  const page = html.replace(SCRIPT, () => '<script type="module" src="{{script}}"></script>');
  if (!/<head>/.test(page)) throw new Error('editor.html: no <head>');
  const hashes = handlerHashes(page);
  const attr = hashes.length > 0 ? `; script-src-attr 'unsafe-hashes' ${hashes.join(' ')}` : '';
  return page.replace(/<head>/, () => `<head>\n<meta http-equiv="Content-Security-Policy" content="{{csp}}${attr}">\n<meta name="ezpzxdw-wasm" content="{{wasm}}">`);
}
