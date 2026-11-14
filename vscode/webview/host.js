// The editor's HOST inside a VS Code webview. scripts/assemble.mjs puts this in the place of the
// upstream web/editor/host.js, which talks to a web page framing the editor (ezpzfile.com): in a
// webview the frame above is VS Code's own, on the same origin, and the upstream file would take
// it for that page. Same members as the upstream HOST (tests/assemble.test.ts checks every one
// app.js uses); `embedded` is false, so the editor looks and behaves as it does on its own.
//
//   editor → extension  ready                    the editor is up: send the document
//                       state {dirty, hasDocument, fileName}
//                       save {name, mime, bytes} a file the editor would have downloaded
//                       print                    the editor asked to print, which a webview cannot do
//   extension → editor  open {name, bytes}       the document of this tab
//                       openFailed               the document could not be read (the extension says why)
//                       savePdf                  save the document as a PDF (offered in place of printing)
//
// The measurements the upstream page counts (event, once, watchErrors) go nowhere.
//
// One member the upstream HOST does not have: loading(stage), a spinner with a line of text. Until the
// document shows, the editor has only its empty screen (the box to drop a file on) with nothing moving,
// and a large binder takes from ten seconds to a minute to arrive and be checked. The spinner hides
// what is in that screen and says what is going on; clicks pass through it.
//   "read"   from the start: the extension reads the document and sends its bytes here
//   "check"  the editor checks a file in a Worker before it opens it (tail.js)
//   null     the document shows, or the editor or the extension says why it could not be opened

const HOST = (() => {
  const vscode = acquireVsCodeApi();
  const listeners = {};
  let last = "";

  const LOADING = {
    read: { ja: "ファイルを読み込んでいます…", en: "Loading the file…" },
    check: { ja: "文書を確かめています…", en: "Checking the document…" },
  };
  const style = document.createElement("style");
  style.textContent = `
#xdw-loading { position: fixed; inset: 0; z-index: 40; display: none; place-items: center; pointer-events: none; }
html.xdw-loading #xdw-loading { display: grid; }
html.xdw-loading #ez-empty > * { visibility: hidden; }
#xdw-loading .card { display: flex; align-items: center; gap: 12px; padding: 14px 20px; border-radius: 12px; background: var(--card, #fff); box-shadow: 0 4px 24px rgba(11, 16, 32, .14); color: var(--ink-700, #364059); font-size: 14px; }
#xdw-loading .spin { width: 22px; height: 22px; box-sizing: border-box; border: 3px solid var(--ink-100, #e3e7ef); border-top-color: var(--brand-500, #0155ff); border-radius: 50%; animation: xdw-spin .8s linear infinite; }
@keyframes xdw-spin { to { transform: rotate(360deg); } }`;
  const spinner = document.createElement("div");
  spinner.id = "xdw-loading";
  spinner.setAttribute("role", "status");
  const card = document.createElement("div");
  card.className = "card";
  const what = document.createElement("span");
  const spin = document.createElement("span");
  spin.className = "spin";
  card.append(spin, what);
  spinner.append(card);
  document.head.append(style);
  document.body.append(spinner);
  function loading(stage) {
    const text = stage ? LOADING[stage] : null;
    document.documentElement.classList.toggle("xdw-loading", !!text);
    if (text) what.textContent = text[LANG] || text.ja;
  }
  // A webview of this extension always has a document to show.
  loading("read");

  window.addEventListener("message", (e) => {
    const d = e.data;
    if (!d || typeof d.type !== "string") return;
    if (d.type === "openFailed") loading(null);
    // The document arrives as bytes; the editor takes a File.
    const data = d.type === "open" ? { file: new File([d.bytes], String(d.name)) } : d;
    for (const fn of listeners[d.type] || []) fn(data);
  });
  return {
    embedded: false,
    errorKind(err) { return err && err.name && err.name !== "Error" ? String(err.name).slice(0, 30) : "other"; },
    ready() { vscode.postMessage({ type: "ready" }); },
    state(s) {
      const key = JSON.stringify(s);
      if (key === last) return;
      last = key;
      vscode.postMessage({ type: "state", ...s });
    },
    event() {},
    once() {},
    resetOnce() {},
    on(what, fn) { (listeners[`${what}`] ||= []).push(fn); },
    navOpen() {},
    homeHref() { return null; },
    async share() {},
    watchErrors() {},
    print() { vscode.postMessage({ type: "print" }); },
    loading,
    /** A download of the editor (.xdw, .xbd, PDF, text): VS Code asks where to save it. */
    save(bytes, name, mime) { vscode.postMessage({ type: "save", name: String(name), mime: String(mime || ""), bytes: new Uint8Array(bytes) }); },
  };
})();
