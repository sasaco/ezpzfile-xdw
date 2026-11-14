// The assembly against the real upstream files (../web), with a stand-in for the wasm-bindgen glue: these fail when an
// upstream change moves something the shell leans on.
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
// @ts-expect-error a plain module of the build
import { CHECK_DRAW, CHECK_MS, editorPage, editorScript, handlerHashes, lf, stripExports } from '../scripts/assemble.mjs';

const root = join(__dirname, '..');
// With the line endings git stores: a checkout on Windows (core.autocrlf) has CRLF.
const read = (...parts: string[]): string => lf(readFileSync(join(root, ...parts), 'utf8'));
const editorHtml = read('..', 'web', 'editor.html');
const app = read('..', 'web', 'editor', 'app.js');
const host = read('webview', 'host.js');
// What the engine throws when it traps (the types of this project do not have WebAssembly as a value).
const Trap = (globalThis as unknown as { WebAssembly: { RuntimeError: new (message: string) => Error } }).WebAssembly.RuntimeError;
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

  it('finds what the spinner leans on: the check before a file opens, with nothing to wait for after it', () => {
    expect(app).toContain('async function checkFirst(bytes) {');
    expect(app).toContain('async function openFile(file) {');
    // Between the check and the document on screen the editor does not wait: the spinner goes with the check.
    expect(app).toMatch(/await checkFirst\(buf\);\n\s*const ed = new XdwDoc\(buf\);\n\s*useDoc\(ed, /);
    expect(script).toContain('\ncheckFirst = async (bytes) => {\n');
    expect(script).toContain('\nopenFile = async (file) => {\n');
    expect(host).toMatch(/^\s+loading,$/m);
    // What the spinner hides while it turns, and the language of its text.
    expect(editorHtml).toContain('<div id="ez-empty">');
    expect(read('..', 'web', 'editor', 'i18n.js')).toMatch(/^const LANG = /m);
  });

  it('puts the same script together from a checkout with CRLF line endings', () => {
    const crlf = (text: string) => text.replace(/\n/g, '\r\n');
    const parts = { editorHtml, glue: GLUE, i18n: read('..', 'web', 'editor', 'i18n.js'), host, app, tail: read('webview', 'tail.js') };
    const windows = Object.fromEntries(Object.entries(parts).map(([key, text]) => [key, crlf(text)]));
    expect(editorScript(windows)).toBe(script);
    expect(editorPage(crlf(editorHtml))).toBe(editorPage(editorHtml));
    expect(script).not.toContain('\r');
  });

  it('shortens the check before a file opens, and fails when upstream changes that line', () => {
    expect(app).toContain('const CHECK_MS = 8000;');
    // The Worker's loop reads it when the check starts.
    expect(app).toContain('drawn < pages && Date.now() - t0 < ${CHECK_MS}; drawn++');
    expect(CHECK_MS).toBeLessThan(8000);
    expect(script).toContain(`\nconst CHECK_MS = ${CHECK_MS};\n`);
    expect(script).not.toContain('const CHECK_MS = 8000;');
    const parts = { editorHtml, glue: GLUE, i18n: '', host, tail: '' };
    expect(() => editorScript({ ...parts, app: app.replace('const CHECK_MS = 8000;', 'const CHECK_MS = 9000;') })).toThrow('CHECK_MS');
  });

  it('lets the check pass a page the engine refuses, and still fail for a trap', () => {
    expect(app).toContain('{ doc.render(drawn); doc.forget(drawn); }');
    expect(script).toContain(`drawn++) ${CHECK_DRAW}\n`);
    expect(script).not.toContain('{ doc.render(drawn); doc.forget(drawn); }');
    // The patched loop as the Worker runs it: a refused page is passed over and forgotten, a trap ends the check.
    const run = (render: (page: number) => void): number[] => {
      const forgotten: number[] = [];
      const doc = { render, forget: (page: number) => forgotten.push(page) };
      new Function('doc', 'pages', `for (let drawn = 0; drawn < pages; drawn++) ${CHECK_DRAW}`)(doc, 4);
      return forgotten;
    };
    expect(run((page) => { if (page === 1) throw new Error('too large or damaged: more than 5000000 points on one page'); })).toEqual([0, 1, 2, 3]);
    expect(() => run((page) => { if (page === 2) throw new Trap('unreachable'); })).toThrow('unreachable');
    const parts = { editorHtml, glue: GLUE, i18n: '', host, tail: '' };
    expect(() => editorScript({ ...parts, app: app.replace('doc.forget(drawn); }', 'doc.forget(drawn) }') })).toThrow('doc.render(drawn)');
  });

  it('finds what the engine wrappers lean on: the search and saving as text read page texts and do not forget', () => {
    expect(app).toContain('if (!pg.drawn && !fold(S.ed.text(i).replace(');
    expect(app).toContain('const t = S.info.map((_, i) => S.ed.text(i)).join("\\n\\f\\n");');
    expect(app).toContain('S.ed.hiddenChars(i)');
    // The editor forgets a page itself after it draws one, by this name.
    expect(app).toContain('S.ed.forget(i);');
    expect(script).toContain('for (const name of Object.getOwnPropertyNames(XdwDoc.prototype)) {');
    expect(script).toContain('const forgets = name === "text" || name === "hiddenChars";');
    expect(script).toContain('if (e instanceof WebAssembly.RuntimeError) HOST.engineStopped();');
    expect(host).toMatch(/^\s+engineStopped,$/m);
  });

  it('wraps the engine: reading a page text forgets the page, a trap is reported, the result is passed on', () => {
    // tail.js against a stand-in for the engine class and the editor's names it touches.
    const calls: string[] = [];
    class XdwDoc {
      text(page: number) { calls.push(`text ${page}`); if (page === 9) throw new Trap('unreachable'); if (page === 8) throw new Error('refused'); return `t${page}`; }
      hiddenChars(page: number) { calls.push(`hidden ${page}`); return 3; }
      render(page: number) { calls.push(`render ${page}`); return '{}'; }
      forget(page: number) { calls.push(`forget ${page}`); }
      free() { calls.push('free'); }
    }
    let stops = 0;
    const HOST = { on() {}, save() {}, print() {}, loading() {}, engineStopped() { stops += 1; } };
    const tail = read('webview', 'tail.js');
    const names = 'let download, checkFirst = async () => {}, openFile = async () => {}, buildPages = () => {}, renderTo = async () => ({}); const S = { pages: [] }; const $ = () => null; const window = {};';
    new Function('XdwDoc', 'HOST', `${names}\n${tail}`)(XdwDoc, HOST);
    const doc = new XdwDoc();
    expect(doc.text(2)).toBe('t2');
    expect(doc.hiddenChars(2)).toBe(3);
    expect(doc.render(5)).toBe('{}');
    doc.free();
    expect(calls).toEqual(['text 2', 'forget 2', 'hidden 2', 'forget 2', 'render 5', 'free']);
    expect(() => doc.text(8)).toThrow('refused');
    expect(stops).toBe(0);
    expect(() => doc.text(9)).toThrow('unreachable');
    expect(stops).toBe(1);
  });

  it('finds what dropping far pages leans on: how the editor draws a page and when', () => {
    // A page that says it is not drawn is drawn again when it comes within 600px; the drop is well outside that.
    expect(app).toContain('function buildPages() {');
    expect(app).toContain('for (const e of ents) if (e.isIntersecting) drawPage(+e.target.dataset.i);');
    expect(app).toContain('{ root: $("#scroller"), rootMargin: "600px 0px" }');
    expect(app).toContain('return { div, canvas, drawn: false, busy: false };');
    expect(app).toContain('if (!pg || pg.busy || (pg.drawn && !force)) return;');
    // The search draws a page that is not drawn before it reads its text layer.
    expect(app).toContain('if (!pg.drawn) await drawPage(i);');
    expect(app).toContain('const layer = el("div", { class: "textlayer" });');
    expect(script).toContain('\nbuildPages = () => {\n');
    const keep = /const PAGE_KEEP = "(\d+)px 0px";/.exec(script);
    expect(Number(keep?.[1])).toBeGreaterThan(600);
  });

  it('finds what the notice of objects not drawn leans on: how the editor draws a page', () => {
    expect(app).toContain('async function renderTo(i, canvas, k) {');
    expect(app).toMatch(/async function renderTo\(i, canvas, k\) \{\n\s*const d = JSON\.parse\(S\.ed\.render\(i\)\);[\s\S]*?\n\s*return d;\n\}/);
    expect(app).toContain('const d = await renderTo(i, pg.canvas, sc());');
    expect(app).toMatch(/^function toast\(msg, ms = \d+\) \{/m);
    expect(script).toContain('\nrenderTo = async (i, canvas, k) => {\n');
  });

  it('says once a page, for the pages drawn together, how many objects the engine left out', async () => {
    const toasts: string[] = [];
    const timers: (() => void)[] = [];
    const drawings: Record<number, { notDrawn?: number }> = { 0: { notDrawn: 2 }, 1: { notDrawn: 0 }, 2: { notDrawn: 1 }, 3: {} };
    const pages = [0, 1, 2, 3].map(() => ({ canvas: {} }));
    const thumb = {};
    const tail = read('webview', 'tail.js');
    const names = 'let download, checkFirst = async () => {}, openFile = async () => {}, buildPages = () => {}; const $ = () => null; const window = {}; class XdwDoc {}; const HOST = { on() {}, loading() {} };';
    // tail.js with stand-ins for the editor's names it touches; the wrapped functions are reached after it ran.
    const run = new Function('S', 'upstream', 'toast', 'setTimeout', 'LANG', `let renderTo = upstream;\n${names}\n${tail}\nreturn { draw: (...a) => renderTo(...a), open: (f) => openFile(f) };`);
    const ui = run({ pages }, async (i: number) => drawings[i], (m: string) => toasts.push(m), (fn: () => void) => { timers.push(fn); return 1; }, 'ja');
    await ui.draw(0, pages[0].canvas, 1);
    await ui.draw(1, pages[1].canvas, 1);
    await ui.draw(2, pages[2].canvas, 1);
    await ui.draw(3, pages[3].canvas, 1);
    // a thumbnail of a page with objects left out says nothing
    await ui.draw(2, thumb, 0.1);
    expect(timers.length).toBe(1);
    timers.shift()?.();
    expect(toasts).toEqual(['1・3 ページに、この画面では表示できないもの（3 か所）があります。図や画像が欠けているところは DocuWorks で見てください。']);
    // drawn again (it went far and came back): nothing more
    await ui.draw(0, pages[0].canvas, 1);
    expect(timers.length).toBe(0);
    // another document: said again
    await ui.open({});
    await ui.draw(0, pages[0].canvas, 1);
    timers.shift()?.();
    expect(toasts.at(-1)).toBe('1 ページに、この画面では表示できないもの（2 か所）があります。図や画像が欠けているところは DocuWorks で見てください。');
    const en = run({ pages }, async (i: number) => drawings[i], (m: string) => toasts.push(m), (fn: () => void) => { fn(); return 1; }, 'en');
    await en.draw(2, pages[2].canvas, 1);
    expect(toasts.at(-1)).toBe('Page 3 has 1 item(s) this screen cannot show. Where a figure or picture is missing, see the document in DocuWorks.');
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
