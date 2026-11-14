import { writeFile } from 'node:fs/promises';
import * as path from 'node:path';
import * as vscode from 'vscode';

import { validateConstraintFormMessage } from '../models/constraintFormMessages';
import { renderConstraintFormHtml } from '../services/constraintFormHtml';
import { formToConstraints, type ConstraintFormState } from '../services/constraintsFormModel';
import type { SchemaValidator } from '../services/schemaValidator';

export interface ConstraintFormPanelOptions {
  extensionUri: vscode.Uri;
  displayPath: string;
  sidecarPath: string;
  initialState: ConstraintFormState;
  validator: SchemaValidator;
  onAnalyze?: (() => Promise<void> | void) | undefined;
}

export class ConstraintFormPanel implements vscode.Disposable {
  public static currentPanel: ConstraintFormPanel | undefined;
  public static readonly viewType = 'bigoLens.constraintForm';

  private readonly panel: vscode.WebviewPanel;
  private readonly options: ConstraintFormPanelOptions;
  private readonly disposables: vscode.Disposable[] = [];
  private isDisposing = false;

  public static createOrShow(options: ConstraintFormPanelOptions): ConstraintFormPanel {
    const column = vscode.window.activeTextEditor ? vscode.ViewColumn.Beside : vscode.ViewColumn.One;

    if (ConstraintFormPanel.currentPanel) {
      ConstraintFormPanel.currentPanel.reveal(column);
      return ConstraintFormPanel.currentPanel;
    }

    const panel = vscode.window.createWebviewPanel(
      ConstraintFormPanel.viewType,
      `Constraints: ${path.basename(options.displayPath)}`,
      column,
      {
        enableScripts: true,
        retainContextWhenHidden: true,
        localResourceRoots: [vscode.Uri.joinPath(options.extensionUri, 'media')],
      },
    );

    ConstraintFormPanel.currentPanel = new ConstraintFormPanel(panel, options);
    return ConstraintFormPanel.currentPanel;
  }

  public constructor(panel: vscode.WebviewPanel, options: ConstraintFormPanelOptions) {
    this.panel = panel;
    this.options = options;

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
        void this.handleMessage(message);
      },
      null,
      this.disposables,
    );
  }

  public reveal(column?: vscode.ViewColumn): void {
    this.panel.reveal(column);
    this.update();
  }

  public update(): void {
    const webview = this.panel.webview;
    const scriptUri = webview.asWebviewUri(
      vscode.Uri.joinPath(this.options.extensionUri, 'media', 'constraintForm.js'),
    );
    const styleUri = webview.asWebviewUri(
      vscode.Uri.joinPath(this.options.extensionUri, 'media', 'constraintForm.css'),
    );

    this.panel.title = `Constraints: ${path.basename(this.options.displayPath)}`;

    webview.html = renderConstraintFormHtml({
      cspSource: webview.cspSource,
      scriptUri: scriptUri.toString(),
      styleUri: styleUri.toString(),
      displayPath: this.options.displayPath,
      sidecarPath: this.options.sidecarPath,
      initialState: this.options.initialState,
    });
  }

  public async handleMessage(raw: unknown): Promise<void> {
    const result = validateConstraintFormMessage(raw);
    if (!result.ok) {
      return;
    }

    const msg = result.message;
    switch (msg.type) {
      case 'ready': {
        // Form loaded and mounted in webview
        break;
      }
      case 'validate': {
        const formOutcome = formToConstraints(msg.state);
        if (!formOutcome.ok) {
          await this.panel.webview.postMessage({
            type: 'validationResult',
            ok: false,
            errors: formOutcome.errors,
          });
          return;
        }

        const schemaOutcome = this.options.validator.validateConstraints(formOutcome.constraints);
        if (!schemaOutcome.value) {
          await this.panel.webview.postMessage({
            type: 'validationResult',
            ok: false,
            errors: schemaOutcome.errors,
          });
          return;
        }

        await this.panel.webview.postMessage({
          type: 'validationResult',
          ok: true,
        });
        break;
      }
      case 'save': {
        const formOutcome = formToConstraints(msg.state);
        if (!formOutcome.ok) {
          await this.panel.webview.postMessage({
            type: 'saveResult',
            ok: false,
            error: formOutcome.errors.join('; '),
          });
          return;
        }

        const schemaOutcome = this.options.validator.validateConstraints(formOutcome.constraints);
        if (!schemaOutcome.value) {
          await this.panel.webview.postMessage({
            type: 'saveResult',
            ok: false,
            error: schemaOutcome.errors.join('; '),
          });
          return;
        }

        try {
          const jsonText = `${JSON.stringify(formOutcome.constraints, null, 2)}\n`;
          await writeFile(this.options.sidecarPath, jsonText, 'utf8');

          await this.panel.webview.postMessage({
            type: 'saveResult',
            ok: true,
            andAnalyze: msg.andAnalyze,
          });

          void vscode.window.showInformationMessage(
            `BigO Lens: Saved constraints to ${path.basename(this.options.sidecarPath)}`,
          );

          if (msg.andAnalyze && this.options.onAnalyze) {
            await this.options.onAnalyze();
          }
        } catch (err) {
          const message = (err as Error).message;
          await this.panel.webview.postMessage({
            type: 'saveResult',
            ok: false,
            error: `Failed to write file: ${message}`,
          });
        }
        break;
      }
    }
  }

  public dispose(): void {
    if (this.isDisposing) {
      return;
    }
    this.isDisposing = true;

    if (ConstraintFormPanel.currentPanel === this) {
      ConstraintFormPanel.currentPanel = undefined;
    }

    this.panel.dispose();

    while (this.disposables.length) {
      const item = this.disposables.pop();
      if (item) {
        item.dispose();
      }
    }
  }
}
