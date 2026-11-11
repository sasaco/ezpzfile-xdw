// After the upstream app.js, in the same module scope (scripts/assemble.mjs). The editor saves
// everything by letting the browser download a file; a webview cannot download, and the file
// belongs in the folder of the document anyway, so the bytes go to the extension instead.
download = (bytes, name, type) => HOST.save(bytes, name, type);

// A webview ignores window.print() (its frame is sandboxed without allow-modals). Printing would do nothing at all, so
// the extension says so and offers the PDF, which prints from any PDF viewer.
window.print = () => HOST.print();
HOST.on("savePdf", () => { if (S.ed) savePdf(); });
