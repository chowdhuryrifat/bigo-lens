import type * as vscode from 'vscode';

import {
  analyzeDocument,
  type AnalyzeDependencies,
  type AnalyzeDocumentOptions,
} from '../commands/analyzeActiveFile';
import { readSettings } from './context';
import { isCppDocument } from './statusBar';

export interface SaveCoordinatorDeps extends AnalyzeDependencies {
  isEnabled?: () => boolean;
  debounceMs?: number;
  analyzeFn?: (
    document: vscode.TextDocument,
    deps: AnalyzeDependencies,
    options?: AnalyzeDocumentOptions,
  ) => Promise<void>;
}

export class SaveAnalysisCoordinator implements vscode.Disposable {
  private readonly fileTimers = new Map<string, NodeJS.Timeout>();
  private readonly fileControllers = new Map<string, AbortController>();
  private readonly deps: AnalyzeDependencies;
  private readonly isEnabled: () => boolean;
  private readonly debounceMs: number;
  private readonly analyzeFn: (
    document: vscode.TextDocument,
    deps: AnalyzeDependencies,
    options?: AnalyzeDocumentOptions,
  ) => Promise<void>;

  constructor(deps: SaveCoordinatorDeps) {
    this.deps = deps;
    this.isEnabled = deps.isEnabled ?? (() => readSettings().analyzeOnSave);
    this.debounceMs = deps.debounceMs ?? 300;
    this.analyzeFn = deps.analyzeFn ?? analyzeDocument;
  }

  public handleDidSave(document: vscode.TextDocument): void {
    if (!this.isEnabled() || !isCppDocument(document)) {
      return;
    }

    const uriStr = document.uri.toString();
    const existingTimer = this.fileTimers.get(uriStr);
    if (existingTimer) {
      clearTimeout(existingTimer);
    }

    const timer = setTimeout(() => {
      void (async () => {
        this.fileTimers.delete(uriStr);

        // Abort any existing in-flight analysis for this file to ensure
        // rapid saves spawn at most one concurrent engine process per file.
        const inFlight = this.fileControllers.get(uriStr);
        if (inFlight) {
          inFlight.abort();
        }

        const abort = new AbortController();
        this.fileControllers.set(uriStr, abort);

        try {
          await this.analyzeFn(document, this.deps, {
            abortSignal: abort.signal,
            showInformation: false,
          });
        } catch {
          // Ignored; errors are logged by analyzeFn
        } finally {
          if (this.fileControllers.get(uriStr) === abort) {
            this.fileControllers.delete(uriStr);
          }
        }
      })();
    }, this.debounceMs);

    this.fileTimers.set(uriStr, timer);
  }

  public get activeTimerCount(): number {
    return this.fileTimers.size;
  }

  public get inFlightCount(): number {
    return this.fileControllers.size;
  }

  public dispose(): void {
    for (const timer of this.fileTimers.values()) {
      clearTimeout(timer);
    }
    this.fileTimers.clear();

    for (const controller of this.fileControllers.values()) {
      controller.abort();
    }
    this.fileControllers.clear();
  }
}
