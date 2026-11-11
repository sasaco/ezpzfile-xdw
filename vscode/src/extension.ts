// The VS Code shell of the EZPZ File XDW editor (structural-mcp Phase 14 stage X, decisions 17-22): a Custom Editor for
// .xdw and .xbd whose webview runs the upstream editor as it is (media/, put together by scripts/build.mjs). No runner:
// the engine is WebAssembly inside the webview.
//
// A readonly Custom Editor: the editor owns the document, its undo and its saving, so VS Code shows no dirty mark and
// does not ask before a tab with unsaved changes closes. What the editor saves (it would download a file in a browser)
// goes through VS Code's save box, which starts in the document's folder; nothing is written until the person picks a name.
import * as vscode from 'vscode';
import { editorHtml } from './html';
import { offeredName, saveFilters, toBytes } from './save';

export const VIEW_TYPE = 'xdwEditor.document';
const OPEN_IN_SCREEN = 'xdwEditor.openInScreen';

class XdwEditorProvider implements vscode.CustomReadonlyEditorProvider {
  constructor(private readonly extensionUri: vscode.Uri) {}

  openCustomDocument(uri: vscode.Uri): vscode.CustomDocument {
    return { uri, dispose: () => {} };
  }

  async resolveCustomEditor(document: vscode.CustomDocument, panel: vscode.WebviewPanel): Promise<void> {
    const media = vscode.Uri.joinPath(this.extensionUri, 'media');
    const webview = panel.webview;
    webview.options = { enableScripts: true, localResourceRoots: [media] };
    webview.onDidReceiveMessage(async (message: unknown) => {
      try {
        await this.handle(document, webview, message);
      } catch (err) {
        void vscode.window.showErrorMessage(`XDW Editor: ${err instanceof Error ? err.message : String(err)}`);
      }
    });
    const template = new TextDecoder().decode(await vscode.workspace.fs.readFile(vscode.Uri.joinPath(media, 'editor.html')));
    webview.html = editorHtml(template, {
      cspSource: webview.cspSource,
      scriptUri: webview.asWebviewUri(vscode.Uri.joinPath(media, 'editor.js')).toString(),
      wasmUri: webview.asWebviewUri(vscode.Uri.joinPath(media, 'ezpzxdw_wasm_bg.wasm')).toString(),
      language: vscode.env.language,
    });
  }

  private async handle(document: vscode.CustomDocument, webview: vscode.Webview, message: unknown): Promise<void> {
    if (typeof message !== 'object' || message === null) return;
    const { type, name, bytes } = message as { type?: unknown; name?: unknown; bytes?: unknown };
    if (type === 'ready') {
      const data = await vscode.workspace.fs.readFile(document.uri);
      // A copy with a buffer of its own: what readFile returns can be a view into a larger one.
      await webview.postMessage({ type: 'open', name: baseName(document.uri), bytes: new Uint8Array(data) });
    } else if (type === 'save') {
      const data = toBytes(bytes);
      if (typeof name !== 'string' || data === null) throw new Error('保存するデータを受け取れませんでした。');
      const offered = offeredName(name);
      const target = await vscode.window.showSaveDialog({ defaultUri: vscode.Uri.joinPath(document.uri, '..', offered), filters: saveFilters(offered) });
      if (target === undefined) {
        void vscode.window.showWarningMessage(`保存しませんでした（${offered}）。`);
        return;
      }
      await vscode.workspace.fs.writeFile(target, data);
      void vscode.window.showInformationMessage(`保存しました: ${target.scheme === 'file' ? target.fsPath : target.toString()}`);
    } else if (type === 'print') {
      // A webview cannot print (VS Code ignores window.print() there).
      const savePdf = 'PDF で保存';
      const picked = await vscode.window.showWarningMessage('VS Code のタブからは印刷できません。PDF で保存して、その PDF を印刷してください。', savePdf);
      if (picked === savePdf) await webview.postMessage({ type: 'savePdf' });
    }
  }
}

function baseName(uri: vscode.Uri): string {
  return uri.path.split('/').pop() ?? '';
}

export function activate(context: vscode.ExtensionContext): void {
  context.subscriptions.push(
    vscode.window.registerCustomEditorProvider(VIEW_TYPE, new XdwEditorProvider(context.extensionUri), {
      // Unsaved changes live in the webview: keep it when the tab is in the background.
      webviewOptions: { retainContextWhenHidden: true },
      supportsMultipleEditorsPerDocument: false,
    }),
    vscode.commands.registerCommand(OPEN_IN_SCREEN, async (uri?: vscode.Uri) => {
      const target = uri ?? vscode.window.activeTextEditor?.document.uri;
      if (target === undefined) {
        void vscode.window.showWarningMessage('XDW Editor で開く .xdw か .xbd を選んでください。');
        return;
      }
      await vscode.commands.executeCommand('vscode.openWith', target, VIEW_TYPE);
    }),
  );
}

export function deactivate(): void {}
