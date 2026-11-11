import { describe, expect, it } from 'vitest';
import { editorHtml, webviewCsp } from '../src/html';
import { offeredName, saveFilters, toBytes } from '../src/save';

describe('editorHtml', () => {
  const template = '<html lang="ja"><head><meta content="{{csp}}"><meta name="ezpzxdw-wasm" content="{{wasm}}"></head><script type="module" src="{{script}}"></script></html>';
  const parts = { cspSource: 'https://host', scriptUri: 'https://host/editor.js', wasmUri: 'https://host/a&b.wasm', language: 'ja' };

  it('fills in the CSP, the URLs and the language', () => {
    const page = editorHtml(template, parts);
    expect(page).toContain('src="https://host/editor.js"');
    expect(page).toContain('content="https://host/a&amp;b.wasm"');
    expect(page).toContain("default-src 'none'");
    expect(page).not.toContain('{{');
    expect(page).toContain('<html lang="ja">');
    expect(editorHtml(template, { ...parts, language: 'en-US' })).toContain('<html lang="en">');
    expect(editorHtml(template, { ...parts, language: 'ko' })).toContain('<html lang="en">');
  });

  it('refuses a template without a mark', () => {
    expect(() => editorHtml(template.replace('{{wasm}}', ''), parts)).toThrow('{{wasm}}');
  });
});

describe('webviewCsp', () => {
  it('lets the engine run and the editor reach only what the upstream editor reaches', () => {
    const csp = webviewCsp('https://host');
    expect(csp).toContain("script-src https://host 'wasm-unsafe-eval' blob: https://cdnjs.cloudflare.com/ajax/libs/pdf.js/3.11.174/");
    expect(csp).toContain('worker-src blob:');
    expect(csp).not.toContain("'unsafe-eval'");
    expect(csp.match(/https:\/\/[a-z.]+/g)?.filter((h) => h !== 'https://host').sort()).toEqual(
      ['https://cdn.jsdelivr.net', 'https://cdn.jsdelivr.net', 'https://cdnjs.cloudflare.com', 'https://cdnjs.cloudflare.com'],
    );
  });
});

describe('toBytes', () => {
  it('takes the forms a webview message can arrive in', () => {
    expect(toBytes(new Uint8Array([1, 2]))).toEqual(new Uint8Array([1, 2]));
    expect(toBytes(new Uint8Array([1, 2]).buffer)).toEqual(new Uint8Array([1, 2]));
    expect(toBytes({ type: 'Buffer', data: [3, 4] })).toEqual(new Uint8Array([3, 4]));
    expect(toBytes({ 0: 5, 1: 255 })).toEqual(new Uint8Array([5, 255]));
  });

  it('refuses anything else', () => {
    for (const bad of [null, undefined, 'abc', 7, { 0: 256 }, { 0: 'x' }, [1.5]]) expect(toBytes(bad)).toBeNull();
  });
});

describe('offeredName', () => {
  it('is never a path', () => {
    expect(offeredName('計算書.xdw')).toBe('計算書.xdw');
    expect(offeredName('../../etc/passwd')).toBe('passwd');
    expect(offeredName('C:\\x\\a.pdf')).toBe('a.pdf');
    expect(offeredName('..')).toBe('document');
    expect(offeredName('a:b?.txt')).toBe('a_b_.txt');
    expect(offeredName('')).toBe('document');
  });
});

describe('saveFilters', () => {
  it('offers the kind the editor made first', () => {
    expect(Object.entries(saveFilters('a.XBD'))).toEqual([['DocuWorks バインダー', ['xbd']], ['すべてのファイル', ['*']]]);
    expect(Object.keys(saveFilters('a.pdf'))[0]).toBe('PDF');
    expect(saveFilters('noext')).toEqual({ 'すべてのファイル': ['*'] });
  });
});
