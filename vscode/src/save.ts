// What the editor hands over to be saved (webview/host.js `save`): a name it made from the document's, and bytes.

/** The bytes of a message from the webview. VS Code hands a Uint8Array over as one; older hosts as plain JSON. */
export function toBytes(value: unknown): Uint8Array | null {
  if (value instanceof Uint8Array) return value;
  if (value instanceof ArrayBuffer) return new Uint8Array(value);
  if (typeof value !== 'object' || value === null) return null;
  const data = (value as { type?: unknown; data?: unknown }).type === 'Buffer' ? (value as { data?: unknown }).data : value;
  const list = Array.isArray(data) ? data : Object.values(data as Record<string, unknown>);
  if (!list.every((n) => typeof n === 'number' && Number.isInteger(n) && n >= 0 && n <= 255)) return null;
  return Uint8Array.from(list as number[]);
}

/** The name to offer in the save box: never a path, whatever the document was called. */
export function offeredName(name: string): string {
  const last = name.split(/[\\/]/).pop() ?? '';
  const clean = last.replace(/[\u0000-\u001f<>:"|?*]/g, '_').replace(/^\.+/, '').trim();
  return clean === '' ? 'document' : clean;
}

const KINDS: Record<string, string> = { xdw: 'DocuWorks 文書', xbd: 'DocuWorks バインダー', pdf: 'PDF', txt: 'テキスト' };

/** The save box's file types: the kind the editor made first, then everything. */
export function saveFilters(name: string): Record<string, string[]> {
  const ext = /\.([A-Za-z0-9]+)$/.exec(name)?.[1]?.toLowerCase();
  const all = { 'すべてのファイル': ['*'] };
  return ext === undefined ? all : { [KINDS[ext] ?? ext]: [ext], ...all };
}
