// Builds the extension: the engine as WebAssembly (cargo + wasm-bindgen, into build/pkg), the upstream editor put
// together for a webview (scripts/assemble.mjs, into media/), src/extension.ts bundled into dist/extension.js, and with
// --package xdw-editor-<version>.vsix (vsce; nothing to install, everything is bundled).
// Needs: rustup target add wasm32-unknown-unknown ; cargo install wasm-bindgen-cli --version 0.2.129 --locked
// --skip-wasm keeps build/pkg from the last build.
import { build } from 'esbuild';
import { cpSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { editorPage, editorScript } from './assemble.mjs';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const repo = join(root, '..');
const web = join(repo, 'web');
const pkg = join(root, 'build', 'pkg');
const media = join(root, 'media');

function run(command, args, cwd) {
  const r = spawnSync(command, args, { cwd, stdio: 'inherit' });
  if (r.error) throw new Error(`${command}: ${r.error.message}`);
  if (r.status !== 0) process.exit(r.status ?? 1);
}

if (!process.argv.includes('--skip-wasm') || !existsSync(join(pkg, 'ezpzxdw_wasm_bg.wasm'))) {
  rmSync(pkg, { recursive: true, force: true });
  run('cargo', ['build', '--release', '--locked', '--target', 'wasm32-unknown-unknown', '-p', 'ezpzxdw-wasm'], join(repo, 'engine'));
  run('wasm-bindgen', ['--target', 'web', '--no-typescript', '--out-dir', pkg, join(repo, 'engine', 'target', 'wasm32-unknown-unknown', 'release', 'ezpzxdw_wasm.wasm')], root);
}

const read = (...parts) => readFileSync(join(...parts), 'utf8');
const editorHtml = read(web, 'editor.html');
rmSync(media, { recursive: true, force: true });
mkdirSync(media, { recursive: true });
writeFileSync(join(media, 'editor.html'), editorPage(editorHtml));
writeFileSync(
  join(media, 'editor.js'),
  editorScript({
    editorHtml,
    glue: read(pkg, 'ezpzxdw_wasm.js'),
    i18n: read(web, 'editor', 'i18n.js'),
    host: read(root, 'webview', 'host.js'),
    app: read(web, 'editor', 'app.js'),
    tail: read(root, 'webview', 'tail.js'),
  }),
);
cpSync(join(pkg, 'ezpzxdw_wasm_bg.wasm'), join(media, 'ezpzxdw_wasm_bg.wasm'));
// The upstream licence and notice travel with the .vsix.
for (const f of ['LICENSE', 'NOTICE']) cpSync(join(repo, f), join(root, f));

await build({
  entryPoints: [join(root, 'src', 'extension.ts')],
  outfile: join(root, 'dist', 'extension.js'),
  bundle: true,
  platform: 'node',
  format: 'cjs',
  target: 'node20',
  external: ['vscode'],
  tsconfig: join(root, 'tsconfig.json'),
  sourcemap: false,
  legalComments: 'none',
  logLevel: 'warning',
});

if (process.argv.includes('--package')) {
  const { version } = JSON.parse(read(root, 'package.json'));
  const out = join(root, `xdw-editor-${version}.vsix`);
  const vsce = join(root, 'node_modules', '@vscode', 'vsce', 'vsce');
  const r = spawnSync(process.execPath, [vsce, 'package', '--no-dependencies', '--out', out], { cwd: root, stdio: 'inherit' });
  if (r.status !== 0) process.exit(r.status ?? 1);
}
