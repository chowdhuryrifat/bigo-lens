import * as vscode from 'vscode';

import { analyzeActiveFile } from './commands/analyzeActiveFile';
import { editConstraints } from './commands/editConstraints';
import { showCapabilities } from './commands/showCapabilities';
import { DiagnosticsPublisher } from './providers/diagnosticsPublisher';
import { ResultsTreeProvider } from './providers/resultsTreeProvider';
import type { ExtensionServices } from './services/context';
import { ensureBundledEngineExecutable } from './services/engineLocator';
import { SchemaValidator } from './services/schemaValidator';

export const COMMANDS = {
  analyzeActiveFile: 'bigoLens.analyzeActiveFile',
  editConstraints: 'bigoLens.editConstraints',
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

  context.subscriptions.push(
    output,
    tree,
    diagnostics,
    vscode.window.registerTreeDataProvider('bigoLens.results', tree),
    vscode.commands.registerCommand(COMMANDS.analyzeActiveFile, () =>
      analyzeActiveFile({ ...services, tree, diagnostics }),
    ),
    vscode.commands.registerCommand(COMMANDS.editConstraints, () => editConstraints()),
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
