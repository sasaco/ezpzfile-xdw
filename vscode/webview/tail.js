// After the upstream app.js, in the same module scope (scripts/assemble.mjs). The editor saves
// everything by letting the browser download a file; a webview cannot download, and the file
// belongs in the folder of the document anyway, so the bytes go to the extension instead.
download = (bytes, name, type) => HOST.save(bytes, name, type);

// A webview ignores window.print() (its frame is sandboxed without allow-modals). Printing would do nothing at all, so
// the extension says so and offers the PDF, which prints from any PDF viewer.
window.print = () => HOST.print();
HOST.on("savePdf", () => { if (S.ed) savePdf(); });

// The editor draws a picture of a page into the unit square that the transform maps to its place (PICTURE_DRAW in
// scripts/assemble.mjs). A picture shown at its own size (not rotated, not flipped, within a pixel of its width and
// height) is put pixel for pixel at the rounded position without smoothing, as the engine's raster puts it, so its
// edges stay sharp. Any other picture is left to the transform, as upstream draws it.
function drawPicture(ctx, img) {
  const t = ctx.getTransform();
  const w = img.width, h = img.height;
  if (t.b === 0 && t.c === 0 && t.a > 0 && t.d > 0 && Math.abs(t.a - w) < 1 && Math.abs(t.d - h) < 1) {
    ctx.save();
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.imageSmoothingEnabled = false;
    ctx.drawImage(img, Math.round(t.e), Math.round(t.f));
    ctx.restore();
    return;
  }
  ctx.drawImage(img, 0, 0, 1, 1);
}

// The spinner (host.js) while the editor checks a file before it opens it (CHECK_MS in scripts/assemble.mjs).
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
  notDrawn.said.clear();
  try { await openFileUpstream(file); } finally { HOST.loading(null); }
};

// The engine leaves out, whole, an object of a page it cannot draw (a picture annotation whose drawing is of a kind
// it does not know, say), and the page shows without it: the editor says nothing, and a figure is simply not there.
// The page's drawing tells how many objects were left out (notDrawn); the first time a page of the document is drawn
// on the screen with some, say so. Pages drawn together (the ones near what shows) go in one message. Thumbnails and
// printing draw through the same function and say nothing.
const NOT_DRAWN = {
  ja: (pages, n) => `${pages.join("・")} ページに、この画面では表示できないもの（${n} か所）があります。図や画像が欠けているところは DocuWorks で見てください。`,
  en: (pages, n) => `${pages.length > 1 ? "Pages" : "Page"} ${pages.join(", ")} ${pages.length > 1 ? "have" : "has"} ${n} item(s) this screen cannot show. Where a figure or picture is missing, see the document in DocuWorks.`,
};
const notDrawn = { said: new Set(), pending: new Map(), timer: 0 };
function sayNotDrawn() {
  notDrawn.timer = 0;
  const pages = [...notDrawn.pending.keys()].sort((a, b) => a - b);
  const n = [...notDrawn.pending.values()].reduce((a, b) => a + b, 0);
  notDrawn.pending.clear();
  if (pages.length) toast((NOT_DRAWN[LANG] || NOT_DRAWN.ja)(pages.map((i) => i + 1), n), 10000);
}
const renderToUpstream = renderTo;
renderTo = async (i, canvas, k) => {
  const d = await renderToUpstream(i, canvas, k);
  const pg = S.pages[i];
  if (d && d.notDrawn > 0 && pg && pg.canvas === canvas && !notDrawn.said.has(i)) {
    notDrawn.said.add(i);
    notDrawn.pending.set(i, d.notDrawn);
    if (!notDrawn.timer) notDrawn.timer = setTimeout(sayNotDrawn, 300);
  }
  return d;
};

// The engine keeps what it worked out for a page until it is told to forget the page. The editor forgets a page after
// it draws it, but not after it reads its text: a search walks through the pages reading their text, saving as text
// reads all of them, and in a binder with scanned pages the engine had used up its 1 GiB after some fifty pages and
// stopped for good. Reading the text of a page (or counting its hidden characters) now forgets the page.
//
// And when the engine does stop (a trap: out of memory, a bug), every call after that fails. The editor would go on
// showing what it had, draw nothing more and save nothing, without a word; HOST says so and offers to open the
// document again.
for (const name of Object.getOwnPropertyNames(XdwDoc.prototype)) {
  const call = Object.getOwnPropertyDescriptor(XdwDoc.prototype, name).value;
  if (typeof call !== "function" || name === "constructor" || name === "free" || name === "forget" || name.startsWith("__")) continue;
  const forgets = name === "text" || name === "hiddenChars";
  XdwDoc.prototype[name] = function (...args) {
    try {
      const out = call.apply(this, args);
      if (forgets) this.forget(args[0]);
      return out;
    } catch (e) {
      if (e instanceof WebAssembly.RuntimeError) HOST.engineStopped();
      throw e;
    }
  };
}

// The editor draws a page when it comes within 600px of what shows (buildPages) and then keeps its canvas and its
// text layer for good: a long document that is read through holds every page it showed (some 8 MB a page on a
// screen at 150%). A second observer gives up the drawing of a page that is far from what shows, well outside those
// 600px. Such a page says it is not drawn, so the editor's own observer draws it again when it comes back, and the
// search draws it when it looks for text there.
const PAGE_KEEP = "2400px 0px";
let farPages = null;
function dropFarPage(pg) {
  if (!pg.far) return;
  // It is being drawn (it was near a moment ago): look again when that is over.
  if (pg.busy) { setTimeout(() => dropFarPage(pg), 500); return; }
  // Nothing to give up: never drawn (the editor sizes a canvas in CSS when it draws it), or given up already.
  // Most pages of a long document are like this when it opens, and touching each of them is what would be slow.
  if (pg.canvas.width === 0 || pg.canvas.style.width === "") return;
  // The text the person selected is in its text layer: keep it until the page next goes far.
  const sel = getSelection();
  if (sel && !sel.isCollapsed && sel.rangeCount > 0 && sel.getRangeAt(0).intersectsNode(pg.div)) return;
  pg.canvas.width = pg.canvas.height = 0;
  const layer = $(".textlayer", pg.div);
  if (layer) layer.remove();
  pg.drawn = false;
}
const buildPagesUpstream = buildPages;
buildPages = () => {
  buildPagesUpstream();
  if (farPages) farPages.disconnect();
  farPages = new IntersectionObserver((ents) => {
    for (const e of ents) {
      const pg = S.pages[+e.target.dataset.i];
      if (!pg || pg.div !== e.target) continue;
      pg.far = !e.isIntersecting;
      dropFarPage(pg);
    }
  }, { root: $("#scroller"), rootMargin: PAGE_KEEP });
  for (const pg of S.pages) farPages.observe(pg.div);
};
