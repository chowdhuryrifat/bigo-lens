import * as path from 'node:path';
import * as vscode from 'vscode';

import { isAnalyzeResults, type Constraints } from '../models/protocol';
import type { DiagnosticsPublisher } from '../providers/diagnosticsPublisher';
import type { ResultsTreeProvider } from '../providers/resultsTreeProvider';
import { constraintsPathFor, loadConstraints } from '../services/constraintsStore';
import { createEngineClient, readSettings, type ExtensionServices } from '../services/context';
import { EngineClient, EngineError } from '../services/engineClient';
import type { ReportSnapshot } from '../services/reportHtml';
import { benchmarkRequest, type BenchmarkInput } from '../services/requestBuilder';
import { normalizeLanguageId, SUPPORTED_LANGUAGES } from './analyzeActiveFile';

export interface BenchmarkDependencies extends ExtensionServices {
  tree: ResultsTreeProvider;
  diagnostics: DiagnosticsPublisher;
  client?: EngineClient | undefined;
  onResults?: (snapshot: ReportSnapshot & { uri: vscode.Uri }) => void;
}

export interface RunBenchmarkOptions {
  abortSignal?: AbortSignal;
  showInformation?: boolean;
  benchmarkConfig?: BenchmarkInput['benchmarkConfig'];
}

/**
 * Returns the transparent isolation level notice for the active platform
 * in accordance with THREAT_MODEL.md §4.2 and BENCHMARK_SPECIFICATION.md §11.
 */
export function getPlatformIsolationNotice(platform: NodeJS.Platform = process.platform): string {
  switch (platform) {
    case 'linux':
      return 'Linux namespaces & seccomp (where supported by host), otherwise not sandboxed (resource limits only)';
    case 'darwin':
      return 'Not sandboxed on this platform (resource limits only; macOS sandbox-exec is deprecated)';
    case 'win32':
      return 'Windows Job Objects (resource limits and kill-on-close; not fully sandboxed)';
    default:
      return 'Not sandboxed on this platform (resource limits only)';
  }
}

/**
 * Runs user-initiated benchmarking with mandatory modal confirmation,
 * workspace trust enforcement, cancellation handling, and results reporting.
 */
export async function runBenchmark(
  deps: BenchmarkDependencies,
  options?: RunBenchmarkOptions,
): Promise<void> {
  // 1. Enforce Workspace Trust (THREAT_MODEL.md §4.2, §4.4)
  if (!vscode.workspace.isTrusted) {
    void vscode.window.showErrorMessage('BigO Lens: benchmarking is disabled in untrusted workspaces.');
    return;
  }

  // 2. Identify target editor / document
  const editor = vscode.window.activeTextEditor;
  if (!editor || !SUPPORTED_LANGUAGES.has(editor.document.languageId)) {
    void vscode.window.showWarningMessage(
      'BigO Lens: open a supported file (C++, Python, Java, Rust, JavaScript, TypeScript) to benchmark.',
    );
    return;
  }

  const document = editor.document;

  // 3. Explicit User Confirmation with transparent isolation wording
  const isolationNotice = getPlatformIsolationNotice();
  const confirmation = await vscode.window.showWarningMessage(
    `BigO Lens: Benchmarking compiles and executes code on your machine.\n\nIsolation level: ${isolationNotice}\n\nDo you want to proceed?`,
    { modal: true },
    'Run Benchmark',
  );

  if (confirmation !== 'Run Benchmark') {
    deps.output.appendLine('BigO Lens: benchmark cancelled by user.');
    return;
  }

  // 4. Create EngineClient
  const client = deps.client ?? createEngineClient(deps);
  if (typeof client === 'string') {
    if (options?.showInformation !== false) {
      void vscode.window.showErrorMessage(`BigO Lens: ${client}`);
    } else {
      deps.output.appendLine(`BigO Lens: ${client}`);
    }
    return;
  }

  // 5. Load sidecar constraints if present
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
      if (options?.showInformation !== false) {
        void vscode.window.showErrorMessage(
          'BigO Lens: the constraints file is invalid; see the BigO Lens output.',
        );
      }
      return;
    }
    deps.diagnostics.clearConstraints(sidecarUri);
    constraints = loaded.kind === 'ok' ? loaded.constraints : undefined;
  }

  const language = normalizeLanguageId(document.languageId);
  const displayPath =
    vscode.workspace.asRelativePath(document.uri, false) || path.basename(document.fileName);

  const request = benchmarkRequest({
    displayPath,
    content: document.getText(),
    constraints,
    language,
    cppStandard: language === 'cpp' ? readSettings().cppStandard : undefined,
    benchmarkConfig: options?.benchmarkConfig,
  });

  const executeRun = async (token?: vscode.CancellationToken): Promise<void> => {
    const abort = new AbortController();
    const sub = token?.onCancellationRequested(() => {
      abort.abort();
    });
    if (options?.abortSignal) {
      if (options.abortSignal.aborted) {
        abort.abort();
      } else {
        options.abortSignal.addEventListener(
          'abort',
          () => {
            abort.abort();
          },
          { once: true },
        );
      }
    }

    try {
      const response = await client.send(request, abort.signal);
      deps.diagnostics.publish(
        document.uri,
        isAnalyzeResults(response.results) ? response.results : null,
        response.diagnostics,
      );

      if (response.status === 'error') {
        const notImplemented = response.errors.find((e) => e.code === 'not_implemented');
        if (notImplemented) {
          deps.output.appendLine(`BigO Lens: ${notImplemented.message}`);
          if (options?.showInformation !== false) {
            void vscode.window.showInformationMessage(`BigO Lens: ${notImplemented.message}`);
          }
          return;
        }

        const message = response.errors.map((e) => `${e.code}: ${e.message}`).join('; ');
        deps.output.appendLine(`Benchmark failed: ${message}`);
        if (options?.showInformation !== false) {
          void vscode.window.showErrorMessage(`BigO Lens: ${message}`);
        }
        return;
      }

      if (isAnalyzeResults(response.results)) {
        const snapshot: ReportSnapshot & { uri: vscode.Uri } = {
          uri: document.uri,
          displayPath,
          results: response.results,
          engineVersion: response.engine.version,
        };
        deps.tree.show({
          uri: document.uri,
          results: response.results,
          engineVersion: response.engine.version,
        });
        deps.onResults?.(snapshot);

        if (response.results.benchmark) {
          void vscode.commands.executeCommand('bigoLens.openReport');
        }
      }
    } catch (err) {
      if (err instanceof EngineError && err.kind === 'cancelled') {
        deps.output.appendLine('BigO Lens: benchmark cancelled by user.');
        if (options?.showInformation !== false) {
          void vscode.window.showInformationMessage('BigO Lens: benchmark cancelled.');
        }
        return;
      }

      const message = err instanceof Error ? err.message : String(err);
      deps.output.appendLine(`Engine failure during benchmark: ${message}`);
      if (err instanceof EngineError && err.stderr) {
        deps.output.appendLine(err.stderr);
      }
      if (options?.showInformation !== false) {
        void vscode.window.showErrorMessage(`BigO Lens: ${message}`);
      }
    } finally {
      sub?.dispose();
    }
  };

  if (options?.showInformation === false) {
    await executeRun();
  } else {
    await vscode.window.withProgress(
      {
        location: vscode.ProgressLocation.Notification,
        title: 'BigO Lens: running benchmark...',
        cancellable: true,
      },
      async (_progress, token) => {
        await executeRun(token);
      },
    );
  }
}
