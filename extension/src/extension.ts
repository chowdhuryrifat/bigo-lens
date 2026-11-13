import * as vscode from 'vscode';

import { analyzeActiveFile } from './commands/analyzeActiveFile';
import { editConstraints } from './commands/editConstraints';
import { openReport } from './commands/openReport';
import { showCapabilities } from './commands/showCapabilities';
import { ReportPanel } from './panels/reportPanel';
import { DiagnosticsPublisher } from './providers/diagnosticsPublisher';
import { ResultsTreeProvider } from './providers/resultsTreeProvider';
import type { ExtensionServices } from './services/context';
import { ensureBundledEngineExecutable } from './services/engineLocator';
import type { ReportSnapshot } from './services/reportHtml';
import { SchemaValidator } from './services/schemaValidator';
import { StatusBarManager } from './services/statusBar';
import { SaveAnalysisCoordinator } from './services/saveAnalysis';

export const COMMANDS = {
  analyzeActiveFile: 'bigoLens.analyzeActiveFile',
  editConstraints: 'bigoLens.editConstraints',
  openReport: 'bigoLens.openReport',
  showCapabilities: 'bigoLens.showCapabilities',
} as const;

export function activate(context: vscode.ExtensionContext): void {
  const output = vscode.window.createOutputChannel('BigO Lens');
  const services: ExtensionServices = {
    extensionRoot: context.extensionPath,
    validator: new SchemaValidator(),
    output,
  };
  const tree = new ResultsTreeProvider();
  const diagnostics = new DiagnosticsPublisher();
  const statusBar = new StatusBarManager();
  let lastSnapshot: ReportSnapshot | undefined;

  statusBar.onActiveEditorChange(vscode.window.activeTextEditor);

  const handleResults = (snapshot: ReportSnapshot & { uri: vscode.Uri }): void => {
    lastSnapshot = snapshot;
    statusBar.updateResults(snapshot.uri, snapshot.results, snapshot.displayPath);
    if (ReportPanel.currentPanel) {
      ReportPanel.currentPanel.updateSnapshot(snapshot);
    }
  };

  const saveCoordinator = new SaveAnalysisCoordinator({
    ...services,
    tree,
    diagnostics,
    onResults: handleResults,
  });

  context.subscriptions.push(
    output,
    tree,
    diagnostics,
    statusBar,
    saveCoordinator,
    vscode.window.onDidChangeActiveTextEditor((editor) => {
      statusBar.onActiveEditorChange(editor);
    }),
    vscode.workspace.onDidSaveTextDocument((document) => {
      saveCoordinator.handleDidSave(document);
    }),
    vscode.window.registerTreeDataProvider('bigoLens.results', tree),
    vscode.commands.registerCommand(COMMANDS.analyzeActiveFile, () =>
      analyzeActiveFile({
        ...services,
        tree,
        diagnostics,
        onResults: handleResults,
      }),
    ),
    vscode.commands.registerCommand(COMMANDS.editConstraints, () => editConstraints()),
    vscode.commands.registerCommand(COMMANDS.openReport, () => {
      openReport(context.extensionUri, () => lastSnapshot);
    }),
    vscode.commands.registerCommand(COMMANDS.showCapabilities, () => showCapabilities(services)),
  );
  if (ensureBundledEngineExecutable(context.extensionPath, process.platform)) {
    output.appendLine('Bundled engine available.');
  }
  output.appendLine('BigO Lens activated.');
}

export function deactivate(): void {
  // All resources are owned by context.subscriptions.
}
