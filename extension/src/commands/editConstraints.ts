import * as vscode from 'vscode';

import { constraintsPathFor, createConstraintsTemplate } from '../services/constraintsStore';

/** Opens (creating from a template if needed) the constraints sidecar of the active file. */
export async function editConstraints(): Promise<void> {
  const editor = vscode.window.activeTextEditor;
  if (!editor || editor.document.languageId !== 'cpp' || editor.document.uri.scheme !== 'file') {
    void vscode.window.showWarningMessage(
      'BigO Lens: open a C++ file saved on disk to edit its constraints.',
    );
    return;
  }
  const sidecar = constraintsPathFor(editor.document.uri.fsPath);
  const created = await createConstraintsTemplate(sidecar);
  const document = await vscode.workspace.openTextDocument(vscode.Uri.file(sidecar));
  await vscode.window.showTextDocument(document, { viewColumn: vscode.ViewColumn.Beside });
  if (created) {
    void vscode.window.showInformationMessage(
      'BigO Lens: created a constraints template. Variable names must match identifiers in your code (e.g. n, t, v.size()).',
    );
  }
}
