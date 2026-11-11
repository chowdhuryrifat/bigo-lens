import * as path from 'node:path';
import * as vscode from 'vscode';

import { isAnalyzeResults, type Constraints } from '../models/protocol';
import type { DiagnosticsPublisher } from '../providers/diagnosticsPublisher';
import type { ResultsTreeProvider } from '../providers/resultsTreeProvider';
import { constraintsPathFor, loadConstraints } from '../services/constraintsStore';
import { createEngineClient, readSettings, type ExtensionServices } from '../services/context';
import { EngineError } from '../services/engineClient';
import { findJsonPointerRange } from '../services/jsonPointer';
import { analyzeRequest } from '../services/requestBuilder';

export interface AnalyzeDependencies extends ExtensionServices {
  tree: ResultsTreeProvider;
  diagnostics: DiagnosticsPublisher;
}

export async function analyzeActiveFile(deps: AnalyzeDependencies): Promise<void> {
  const editor = vscode.window.activeTextEditor;
  if (!editor || (editor.document.languageId !== 'cpp' && editor.document.languageId !== 'python')) {
    void vscode.window.showWarningMessage('BigO Lens: open a supported file (Python or C++) to analyze.');
    return;
  }
  const document = editor.document;
  const client = createEngineClient(deps);
  if (typeof client === 'string') {
    void vscode.window.showErrorMessage(`BigO Lens: ${client}`);
    return;
  }

  let constraints: Constraints | undefined;
  let sidecarUri: vscode.Uri | undefined;
  if (document.uri.scheme === 'file') {
    const sidecar = constraintsPathFor(document.uri.fsPath);
    sidecarUri = vscode.Uri.file(sidecar);
    const loaded = await loadConstraints(sidecar, deps.validator);
    if (loaded.kind === 'invalid') {
      deps.diagnostics.publishConstraints(sidecarUri, loaded.diagnostics);
      deps.output.appendLine(`Invalid constraints in ${sidecar}:`);
      loaded.errors.forEach((e) => {
        deps.output.appendLine(`  ${e}`);
      });
      deps.output.show(true);
      void vscode.window.showErrorMessage(
        'BigO Lens: the constraints file is invalid; see the BigO Lens output.',
      );
      return;
    }
    deps.diagnostics.clearConstraints(sidecarUri);
    constraints = loaded.kind === 'ok' ? loaded.constraints : undefined;
  }

  const language = document.languageId === 'python' ? 'python' : 'cpp';
  const displayPath =
    vscode.workspace.asRelativePath(document.uri, false) || path.basename(document.fileName);
  const request = analyzeRequest({
    displayPath,
    content: document.getText(),
    constraints,
    language,
    cppStandard: language === 'cpp' ? readSettings().cppStandard : undefined,
  });

  await vscode.window.withProgress(
    { location: vscode.ProgressLocation.Window, title: 'BigO Lens: analyzing', cancellable: true },
    async (_progress, token) => {
      const abort = new AbortController();
      const subscription = token.onCancellationRequested(() => {
        abort.abort();
      });
      try {
        const response = await client.send(request, abort.signal);
        deps.diagnostics.publish(
          document.uri,
          isAnalyzeResults(response.results) ? response.results : null,
          response.diagnostics,
        );
        if (response.status === 'error') {
          if (sidecarUri) {
            const constraintsErrors = response.errors.filter(
              (e): e is typeof e & { path: string } =>
                typeof e.path === 'string' && e.path.startsWith('/constraints'),
            );
            if (constraintsErrors.length > 0) {
              try {
                const sidecarBytes = await vscode.workspace.fs.readFile(sidecarUri);
                const sidecarText = new TextDecoder().decode(sidecarBytes);
                const sidecarDiagnostics = constraintsErrors.map((e) => {
                  const relPath = e.path.replace(/^\/constraints/, '');
                  return {
                    message: e.message,
                    path: relPath,
                    range: findJsonPointerRange(sidecarText, relPath),
                  };
                });
                deps.diagnostics.publishConstraints(sidecarUri, sidecarDiagnostics);
              } catch {
                // Ignore failure to read sidecar text
              }
            }
          }
          const message = response.errors.map((e) => `${e.code}: ${e.message}`).join('; ');
          deps.output.appendLine(`Analysis failed: ${message}`);
          deps.tree.show(undefined);
          void vscode.window.showErrorMessage(`BigO Lens: ${message}`);
          return;
        }
        if (isAnalyzeResults(response.results)) {
          deps.tree.show({
            uri: document.uri,
            results: response.results,
            engineVersion: response.engine.version,
          });
          if (!constraints) {
            void vscode.window.showInformationMessage(
              'BigO Lens: analysis done without constraints, so workloads are unknown. Use "Edit Constraints" to add them.',
            );
          }
        }
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        deps.output.appendLine(`Engine failure: ${message}`);
        if (err instanceof EngineError && err.stderr) {
          deps.output.appendLine(err.stderr);
        }
        if (!(err instanceof EngineError && err.kind === 'cancelled')) {
          void vscode.window.showErrorMessage(`BigO Lens: ${message}`);
        }
      } finally {
        subscription.dispose();
      }
    },
  );
}
