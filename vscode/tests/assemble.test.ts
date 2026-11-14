// The assembly against the real upstream files (../web), with a stand-in for the wasm-bindgen glue: these fail when an
// upstream change moves something the shell leans on.
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
// @ts-expect-error a plain module of the build
import { editorPage, editorScript, handlerHashes, stripExports } from '../scripts/assemble.mjs';

const root = join(__dirname, '..');
const read = (...parts: string[]) => readFileSync(join(root, ...parts), 'utf8');
const editorHtml = read('..', 'web', 'editor.html');
const app = read('..', 'web', 'editor', 'app.js');
const host = read('webview', 'host.js');
const GLUE = 'export class XdwDoc {}\nasync function __wbg_init() {}\nexport { __wbg_init as default };\n';
const script: string = editorScript({ editorHtml, glue: GLUE, i18n: read('..', 'web', 'editor', 'i18n.js'), host, app, tail: read('webview', 'tail.js') });

describe('the editor script', () => {
  it('parses as one module without exports', () => {
    expect(stripExports(GLUE)).toBe('class XdwDoc {}\nasync function __wbg_init() {}\n');
    expect(script).not.toMatch(/^export /m);
    // A syntax error anywhere in the assembled text throws here.
    expect(() => new Function(`return async () => {${script}\n}`)).not.toThrow();
  });

  it('leaves no placeholder but the inline wasm, which stays unused', () => {
    expect(script.match(/__EZPZXDW_[A-Z_]+__/g)).toEqual(['__EZPZXDW_WASM__']);
    expect(script).toContain('const WASM_URL = document.querySelector(\'meta[name="ezpzxdw-wasm"]\').content;');
    expect(script).toContain(`const GLUE_SRC = ${JSON.stringify(stripExports(GLUE))};`);
  });

  it('uses the webview host, not the upstream one that talks to a framing page', () => {
    expect(script).toContain('acquireVsCodeApi()');
    expect(script).not.toContain('window.parent');
  });

  it('gives the editor every HOST member it uses', () => {
    const used = new Set([...app.matchAll(/\bHOST\.([A-Za-z]+)/g)].map((m) => m[1]));
    expect(used.size).toBeGreaterThan(5);
    for (const member of used) expect(host, `HOST.${member}`).toMatch(new RegExp(`^\\s+(async )?${member}[(:]`, 'm'));
  });

  it('finds the download function it replaces, and every save still goes through it', () => {
    expect(app).toContain('function download(bytes, name, type) {');
    expect(app.match(/createObjectURL/g)?.length).toBeGreaterThan(0);
    expect(script).toContain('\ndownload = (bytes, name, type) => HOST.save(bytes, name, type);\n');
  });

  it('finds what the print bridge leans on', () => {
    expect(app).toContain('window.print();');
    expect(app).toContain('async function savePdf(name) {');
    expect(app).toMatch(/^const S = \{/m);
  });

  it('keeps the one change this fork makes to the upstream editor: thumbnails are drawn only near what shows', () => {
    // Without it a binder of thousands of pages draws, and keeps, a canvas for every page (README.md).
    expect(app).toContain('S.thumbObserver = new IntersectionObserver(');
    expect(app).toContain('const i = S.thumbs.findIndex((t) => t.near && !t.done);');
    expect(app).not.toMatch(/for \(let i = 0; i < S\.thumbs\.length; i\+\+\)/);
  });

  it('still opens a File handed over by the host', () => {
    expect(app).toContain('HOST.on("open", (d) => { if (d.file instanceof File) openFile(d.file); });');
  });
});

describe('the editor page', () => {
  const page: string = editorPage(editorHtml);
  it('loads the script as a file and has the marks the extension fills in', () => {
    expect(page).toContain('<script type="module" src="{{script}}"></script>');
    expect(page).not.toContain('/*__EZPZXDW_');
    for (const mark of ['{{csp}}', '{{wasm}}', '{{script}}']) expect(page.split(mark).length, mark).toBe(2);
    expect(page).toContain('<html lang="ja">');
  });

  it('has no other script, and lets its own inline handlers run by their hashes', () => {
    expect(page.match(/<script/g)?.length).toBe(1);
    const hashes: string[] = handlerHashes(editorHtml);
    expect(hashes.length).toBe(new Set(editorHtml.match(/\son[a-z]+="[^"]*"/g)).size);
    expect(page).toContain(`content="{{csp}}; script-src-attr 'unsafe-hashes' ${hashes.join(' ')}">`);
    // sha256 of the text `a()`, as a browser computes it for onclick="a()"
    expect(handlerHashes('<b onclick="a()">')).toEqual(["'sha256-qVpDBgj7bpq5hMAcGp3AOc79J3Y1Z4HvySTwKrWDoy4='"]);
  });
});
