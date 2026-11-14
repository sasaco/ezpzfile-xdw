// Puts the upstream editor (../web: editor.html, editor/i18n.js, editor/app.js and the wasm-bindgen
// glue) together for a VS Code webview, as web/build.sh does for a web page. The differences:
//   - the module script is a file of its own (a webview's CSP then needs no inline script),
//   - the .wasm is loaded from the URL in <meta name="ezpzxdw-wasm"> (the extension fills it in),
//   - webview/host.js stands in for editor/host.js, and webview/tail.js follows app.js.
// This changes no upstream file (the fork's one change to them is in web/editor/app.js: thumbnails are
// drawn only near what shows; ../README.md). Pure functions on text, so tests run them without a build.

import { createHash } from 'node:crypto';

const SCRIPT = /<script type="module">\n([\s\S]*?)<\/script>/;

/** The glue without its exports, as build.sh strips them: its names become the script's own. */
export function stripExports(glue) {
  return glue.replace(/^export \{[^}]*\};?\s*$/gm, '').replace(/^export (class|function|async function|const|let) /gm, '$1 ');
}

function replaceOnce(text, find, value, what) {
  const at = text.indexOf(find);
  if (at < 0 || text.indexOf(find, at + find.length) >= 0) throw new Error(`editor.html: expected ${what} exactly once`);
  return text.slice(0, at) + value + text.slice(at + find.length);
}

/** The editor's module script. */
export function editorScript({ editorHtml, glue, i18n, host, app, tail }) {
  const found = SCRIPT.exec(editorHtml);
  if (!found) throw new Error('editor.html: no <script type="module">');
  const stripped = stripExports(glue);
  let script = found[1];
  script = replaceOnce(script, '/*__EZPZXDW_GLUE__*/', stripped, 'the glue placeholder');
  script = replaceOnce(script, '"/*__EZPZXDW_WASM_URL__*/"', 'document.querySelector(\'meta[name="ezpzxdw-wasm"]\').content', 'the .wasm URL placeholder');
  script = replaceOnce(script, '/*__EZPZXDW_GLUE_STR__*/""', JSON.stringify(stripped), 'the glue string placeholder');
  script = replaceOnce(script, '/*__EZPZXDW_APP__*/', [i18n, host, app, tail].join('\n'), 'the app placeholder');
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
  if (!SCRIPT.test(editorHtml)) throw new Error('editor.html: no <script type="module">');
  const page = editorHtml.replace(SCRIPT, () => '<script type="module" src="{{script}}"></script>');
  if (!/<head>/.test(page)) throw new Error('editor.html: no <head>');
  const hashes = handlerHashes(page);
  const attr = hashes.length > 0 ? `; script-src-attr 'unsafe-hashes' ${hashes.join(' ')}` : '';
  return page.replace(/<head>/, () => `<head>\n<meta http-equiv="Content-Security-Policy" content="{{csp}}${attr}">\n<meta name="ezpzxdw-wasm" content="{{wasm}}">`);
}
