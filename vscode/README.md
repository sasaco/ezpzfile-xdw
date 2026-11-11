# XDW Editor（VS Code の拡張、`vscode/`）

[EZPZ File XDW](https://github.com/ezpzfile/ezpzfile-xdw) のエディタ（`../web`）を、VS Code の Custom Editor の webview で動かす拡張です。DocuWorks の文書（`.xdw`）とバインダー（`.xbd`）を VS Code（ブラウザの code-server を含む）のタブで開き、見て、アノテーションを付け、保存します。

富士フイルムビジネスイノベーション株式会社とは関係ありません。DocuWorks は同社の商標です。

Marketplace・Open VSX には載せていません。この repo の release の `.vsix` を手で入れます。

## すること

- `.xdw` と `.xbd` を既定でこの画面に開く（Custom Editor `xdwEditor.document`、Explorer の右クリック「XDW Editor で開く」、コマンド `xdwEditor.openInScreen`、ターミナルからは `code <ファイル>`）。
- 上流のエディタを変えずに載せる。できることは上流と同じ：ページをめくる、文字の選択・コピー・検索、アノテーション、ページの操作、`.xdw`／`.xbd` の保存、PDF・テキストの書き出し。エンジンは webview の中の WebAssembly で、ファイルは外に送られない。
- **保存**：上流のエディタはファイルをブラウザのダウンロードとして保存する。この拡張では、そのバイトを VS Code の保存の画面に渡す。開いている文書のフォルダと、エディタが付けた名前を勧める。元と同じ名前を選ぶと上書きで、VS Code が確かめる。名前を選ぶまで何も書かない。
- **印刷**：VS Code の webview は `window.print()` を無視するので、印刷はできない。エディタの印刷を押すと、その旨を知らせて「PDF で保存」を勧める（保存した PDF を印刷する）。
- 画面の言葉は VS Code の表示言語に合わせる（日本語なら日本語、ほかは英語）。
- 読み取り専用の Custom Editor：文書・元に戻す・保存はエディタが持つので、VS Code の未保存の印は出ず、**保存していない変更があるタブを閉じても VS Code は確かめない**。
- 開いている文書をほかで書き換えても、画面は気付かない。開き直せば新しい中身になる。
- 外のネットに出るのは上流と同じ 2 つだけ：画面の書体の CSS（jsdelivr の Pretendard。取れなければ PC の書体）と、PDF のページを挿入するときの pdf.js（cdnjs）。webview の CSP はこの 2 つだけを通す。

## 作り

| | |
|---|---|
| `scripts/assemble.mjs` | 上流の `web/editor.html`・`editor/i18n.js`・`editor/app.js` と wasm-bindgen の glue を、webview 用に組み立てる（上流の `web/build.sh` と同じ組み立て）。スクリプトは別ファイルにし、`.wasm` は URL から読む |
| `webview/host.js` | 上流の `editor/host.js` の代わりに入れる `HOST`。上流のものはエディタを枠に入れた Web ページ（ezpzfile.com）と話すが、webview では上の枠が VS Code 自身なので、それをそのページと見誤る。同じ形で、文書の受け取りと保存を拡張とやりとりする |
| `webview/tail.js` | `app.js` の後ろに足す数行。エディタの `download` を拡張に渡すものに差し替え、`window.print` を上の案内に差し替える |
| `src/extension.ts` | Custom Editor。文書のバイトを webview に渡し、保存を受けて保存の画面を出す |
| `src/html.ts`・`src/save.ts` | webview の CSP とページ、保存の名前と種類 |

上流のファイルは 1 つも変えていません。上流が `HOST` の形や `download` を変えたら `tests/assemble.test.ts` が落ちます。

## ビルドとテスト

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129 --locked   # engine/Cargo.lock の wasm-bindgen と同じ版
npm --prefix vscode ci
npm --prefix vscode run typecheck
npm --prefix vscode test
npm --prefix vscode run package    # vscode/xdw-editor-<版>.vsix（--skip-wasm で前の WASM を使う）
```

release：タグ `vscode-v<package.json の version>` を push すると、`.github/workflows/vscode-release.yml` が `.vsix` と `SHA256SUMS.txt` を GitHub release に出します。

## 入れ方

```sh
code-server --extensions-dir <dir> --install-extension xdw-editor-<版>.vsix
```

## 確かめたこと

2026-11-11 に code-server 4.145.2（macOS、Intel、Chrome）で、設計計算書の `.xdw`（1.8 MB、32 ページ）と `.xbd`（12.9 MB、561 ページ）を使って確かめた：既定で画面に開く、ページの描画と文字の層、検索、文字の選択、四角形のアノテーション、保存（保存の画面が文書のフォルダと名前を勧め、別の名前で書ける。元と同じ名前では VS Code が上書きを確かめる。取り消すと何も書かない）、PDF の書き出し、PDF のページの挿入（cdnjs の pdf.js）、印刷の案内から PDF の保存。どの操作でも、保存の画面で名前を選ぶまで元のファイルのバイトは変わらない。

まだ確かめていないこと：Windows の code-server、50 MB 級のバインダー、本物の保護された文書の案内、ファイルのドロップ、日付印・付箋・画像のアノテーション、`code <ファイル>` で開くこと。

## ライセンス

MIT（`../LICENSE`、`../NOTICE`。`.vsix` にも入ります）。
