// After the upstream app.js, in the same module scope (scripts/assemble.mjs). The editor saves
// everything by letting the browser download a file; a webview cannot download, and the file
// belongs in the folder of the document anyway, so the bytes go to the extension instead.
download = (bytes, name, type) => HOST.save(bytes, name, type);

// A webview ignores window.print() (its frame is sandboxed without allow-modals). Printing would do nothing at all, so
// the extension says so and offers the PDF, which prints from any PDF viewer.
window.print = () => HOST.print();
HOST.on("savePdf", () => { if (S.ed) savePdf(); });

// The spinner (host.js) while the editor checks a file before it opens it: up to eight seconds for a long document.
// After the check nothing waits any more (the engine opens the bytes and the pages are laid out before the screen is
// drawn again), so the spinner goes when the document shows, or when the editor says why it could not open it.
const checkFirstUpstream = checkFirst;
checkFirst = async (bytes) => {
  HOST.loading("check");
  try { await checkFirstUpstream(bytes); } finally { HOST.loading(null); }
};
// However opening ends, also before it comes to the check, the spinner of the first load goes.
const openFileUpstream = openFile;
openFile = async (file) => {
  try { await openFileUpstream(file); } finally { HOST.loading(null); }
};
