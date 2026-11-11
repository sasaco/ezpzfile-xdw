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
//                       savePdf                  save the document as a PDF (offered in place of printing)
//
// The measurements the upstream page counts (event, once, watchErrors) go nowhere.

const HOST = (() => {
  const vscode = acquireVsCodeApi();
  const listeners = {};
  let last = "";
  window.addEventListener("message", (e) => {
    const d = e.data;
    if (!d || typeof d.type !== "string") return;
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
    /** A download of the editor (.xdw, .xbd, PDF, text): VS Code asks where to save it. */
    save(bytes, name, mime) { vscode.postMessage({ type: "save", name: String(name), mime: String(mime || ""), bytes: new Uint8Array(bytes) }); },
  };
})();
