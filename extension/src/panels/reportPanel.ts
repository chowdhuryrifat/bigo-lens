import * as path from 'node:path';
import * as vscode from 'vscode';

import { validateWebviewMessage } from '../models/webview';
import type { ReportSnapshot } from '../services/reportHtml';
import { renderReportHtml } from '../services/reportHtml';

export class ReportPanel implements vscode.Disposable {
  public static currentPanel: ReportPanel | undefined;
  public static readonly viewType = 'bigoLens.report';

  private readonly panel: vscode.WebviewPanel;
  private readonly extensionUri: vscode.Uri;
  private readonly disposables: vscode.Disposable[] = [];
  private snapshot?: ReportSnapshot | undefined;
  private isDisposing = false;

  public static createOrShow(extensionUri: vscode.Uri, snapshot?: ReportSnapshot): ReportPanel {
    const column = vscode.window.activeTextEditor ? vscode.ViewColumn.Beside : vscode.ViewColumn.One;

    if (ReportPanel.currentPanel) {
      ReportPanel.currentPanel.reveal(column, snapshot);
      return ReportPanel.currentPanel;
    }

    const panel = vscode.window.createWebviewPanel(ReportPanel.viewType, 'BigO Lens Report', column, {
      enableScripts: true,
      retainContextWhenHidden: true,
      localResourceRoots: [vscode.Uri.joinPath(extensionUri, 'media')],
    });

    ReportPanel.currentPanel = new ReportPanel(panel, extensionUri, snapshot);
    return ReportPanel.currentPanel;
  }

  public constructor(panel: vscode.WebviewPanel, extensionUri: vscode.Uri, snapshot?: ReportSnapshot) {
    this.panel = panel;
    this.extensionUri = extensionUri;
    this.snapshot = snapshot;

    this.update();

    this.panel.onDidDispose(
      () => {
        this.dispose();
      },
      null,
      this.disposables,
    );

    this.panel.webview.onDidReceiveMessage(
      (message: unknown) => {
        this.handleMessage(message);
      },
      null,
      this.disposables,
    );
  }

  public reveal(column?: vscode.ViewColumn, snapshot?: ReportSnapshot): void {
    if (snapshot) {
      this.snapshot = snapshot;
    }
    this.panel.reveal(column);
    this.update();
  }

  public updateSnapshot(snapshot: ReportSnapshot): void {
    this.snapshot = snapshot;
    this.update();
  }

  public update(): void {
    const webview = this.panel.webview;
    const scriptUri = webview.asWebviewUri(vscode.Uri.joinPath(this.extensionUri, 'media', 'report.js'));
    const styleUri = webview.asWebviewUri(vscode.Uri.joinPath(this.extensionUri, 'media', 'report.css'));

    this.panel.title = this.snapshot
      ? `Report: ${path.basename(this.snapshot.displayPath)}`
      : 'BigO Lens Report';

    webview.html = renderReportHtml({
      cspSource: webview.cspSource,
      scriptUri: scriptUri.toString(),
      styleUri: styleUri.toString(),
      snapshot: this.snapshot,
    });
  }

  public handleMessage(raw: unknown): void {
    const result = validateWebviewMessage(raw);
    if (!result.ok) {
      // Rejects unknown message types or malformed payloads
      return;
    }

    const msg = result.message;
    if (msg.type === 'navigate') {
      void this.navigateToLocation(msg.location);
    }
  }

  private async navigateToLocation(loc: { file?: string; line: number; column?: number }): Promise<void> {
    try {
      let targetUri: vscode.Uri | undefined;
      if (loc.file) {
        if (path.isAbsolute(loc.file)) {
          targetUri = vscode.Uri.file(loc.file);
        } else {
          const workspaceFolder = vscode.workspace.workspaceFolders?.[0];
          if (workspaceFolder) {
            targetUri = vscode.Uri.joinPath(workspaceFolder.uri, loc.file);
          } else if (vscode.window.activeTextEditor) {
            const dir = path.dirname(vscode.window.activeTextEditor.document.uri.fsPath);
            targetUri = vscode.Uri.file(path.join(dir, loc.file));
          }
        }
      } else if (vscode.window.activeTextEditor) {
        targetUri = vscode.window.activeTextEditor.document.uri;
      }

      if (!targetUri) {
        return;
      }

      const document = await vscode.workspace.openTextDocument(targetUri);
      const editor = await vscode.window.showTextDocument(document, {
        viewColumn: vscode.ViewColumn.One,
      });

      const zeroLine = Math.max(0, loc.line - 1);
      const zeroCol = Math.max(0, (loc.column ?? 1) - 1);
      const pos = new vscode.Position(zeroLine, zeroCol);
      editor.selection = new vscode.Selection(pos, pos);
      editor.revealRange(new vscode.Range(pos, pos), vscode.TextEditorRevealType.InCenter);
    } catch {
      // Ignore failure to navigate to deleted or unreadable files
    }
  }

  public dispose(): void {
    if (this.isDisposing) {
      return;
    }
    this.isDisposing = true;
    if (ReportPanel.currentPanel === this) {
      ReportPanel.currentPanel = undefined;
    }
    this.panel.dispose();
    while (this.disposables.length) {
      const d = this.disposables.pop();
      if (d) {
        d.dispose();
      }
    }
  }
}
