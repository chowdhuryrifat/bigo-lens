import assert from 'node:assert/strict';
import Module from 'node:module';
import { describe, it } from 'node:test';

// Intercept 'vscode' import in Node.js test environment
type ResolveFn = (request: string, parent: unknown, isMain: boolean, options?: unknown) => string;

interface ModuleWithResolve {
  _resolveFilename: ResolveFn;
}

const moduleObj = Module as unknown as ModuleWithResolve;
const originalResolve = moduleObj._resolveFilename;
moduleObj._resolveFilename = function (
  request: string,
  parent: unknown,
  isMain: boolean,
  options?: unknown,
): string {
  if (request === 'vscode') {
    return require.resolve('../mocks/mockVscode');
  }
  return originalResolve.call(this, request, parent, isMain, options);
};

import type * as vscode from 'vscode';

import { SaveAnalysisCoordinator } from '../../src/services/saveAnalysis';
import { MockUri } from '../mocks/mockVscode';

function makeMockDoc(path: string, languageId = 'cpp'): vscode.TextDocument {
  return {
    uri: MockUri.file(path) as unknown as vscode.Uri,
    languageId,
    getText: () => 'int main() {}',
  } as vscode.TextDocument;
}

function makeMockDeps() {
  return {
    extensionRoot: '/mock',
    output: { appendLine: () => {}, show: () => {} } as unknown as vscode.OutputChannel,
    validator: {} as unknown as import('../../src/services/schemaValidator').SchemaValidator,
    tree: {} as unknown as import('../../src/providers/resultsTreeProvider').ResultsTreeProvider,
    diagnostics: {} as unknown as import('../../src/providers/diagnosticsPublisher').DiagnosticsPublisher,
  };
}

describe('SaveAnalysisCoordinator', () => {
  it('does nothing when analyzeOnSave is disabled (default)', async () => {
    let analyzed = false;
    const coordinator = new SaveAnalysisCoordinator({
      ...makeMockDeps(),
      isEnabled: () => false,
      debounceMs: 10,
      analyzeFn: () => {
        analyzed = true;
        return Promise.resolve();
      },
    });

    const doc = makeMockDoc('/workspace/solution.cpp', 'cpp');
    coordinator.handleDidSave(doc);

    await new Promise((resolve) => setTimeout(resolve, 30));
    assert.equal(analyzed, false);
    assert.equal(coordinator.activeTimerCount, 0);
    coordinator.dispose();
  });

  it('does nothing for non-C++ files even when enabled', async () => {
    let analyzed = false;
    const coordinator = new SaveAnalysisCoordinator({
      ...makeMockDeps(),
      isEnabled: () => true,
      debounceMs: 10,
      analyzeFn: () => {
        analyzed = true;
        return Promise.resolve();
      },
    });

    const doc = makeMockDoc('/workspace/script.py', 'python');
    coordinator.handleDidSave(doc);

    await new Promise((resolve) => setTimeout(resolve, 30));
    assert.equal(analyzed, false);
    assert.equal(coordinator.activeTimerCount, 0);
    coordinator.dispose();
  });

  it('debounces rapid saves on the same file, invoking analyzeFn only once', async () => {
    let callCount = 0;
    const coordinator = new SaveAnalysisCoordinator({
      ...makeMockDeps(),
      isEnabled: () => true,
      debounceMs: 40,
      analyzeFn: () => {
        callCount++;
        return Promise.resolve();
      },
    });

    const doc = makeMockDoc('/workspace/solution.cpp', 'cpp');

    // Rapid saves in quick succession
    coordinator.handleDidSave(doc);
    coordinator.handleDidSave(doc);
    coordinator.handleDidSave(doc);

    assert.equal(coordinator.activeTimerCount, 1);

    await new Promise((resolve) => setTimeout(resolve, 80));
    assert.equal(callCount, 1);
    assert.equal(coordinator.activeTimerCount, 0);
    coordinator.dispose();
  });

  it('cancels in-flight engine process when a new save occurs and spawns at most one concurrent engine', async () => {
    const signals: AbortSignal[] = [];
    let resolveFirstInFlight: () => void = () => {};
    const firstInFlightPromise = new Promise<void>((res) => {
      resolveFirstInFlight = res;
    });

    let executionCount = 0;

    const coordinator = new SaveAnalysisCoordinator({
      ...makeMockDeps(),
      isEnabled: () => true,
      debounceMs: 20,
      analyzeFn: async (_doc, _deps, options) => {
        executionCount++;
        const signal = options?.abortSignal;
        if (signal) {
          signals.push(signal);
        }
        if (executionCount === 1) {
          // Simulate slow in-flight engine
          await firstInFlightPromise;
        }
      },
    });

    const doc = makeMockDoc('/workspace/solution.cpp', 'cpp');

    // Save #1 triggers first analysis
    coordinator.handleDidSave(doc);
    await new Promise((resolve) => setTimeout(resolve, 35));

    assert.equal(executionCount, 1);
    assert.equal(signals.length, 1);
    assert.equal(signals[0]?.aborted, false);
    assert.equal(coordinator.inFlightCount, 1);

    // Save #2 occurs while save #1 is still in flight
    coordinator.handleDidSave(doc);
    await new Promise((resolve) => setTimeout(resolve, 35));

    // First in-flight analysis must have been aborted
    const firstSignal = signals[0];
    assert.ok(firstSignal);
    assert.equal(firstSignal.aborted, true);
    assert.equal(executionCount, 2);
    assert.equal(signals.length, 2);
    const secondSignal = signals[1];
    assert.ok(secondSignal);
    assert.equal(secondSignal.aborted, false);

    // Allow first to resolve
    resolveFirstInFlight();
    await new Promise((resolve) => setTimeout(resolve, 10));

    coordinator.dispose();
  });

  it('tracks multiple files independently', async () => {
    const analyzedFiles: string[] = [];
    const coordinator = new SaveAnalysisCoordinator({
      ...makeMockDeps(),
      isEnabled: () => true,
      debounceMs: 20,
      analyzeFn: (doc) => {
        analyzedFiles.push(doc.uri.fsPath);
        return Promise.resolve();
      },
    });

    const doc1 = makeMockDoc('/workspace/file1.cpp', 'cpp');
    const doc2 = makeMockDoc('/workspace/file2.cpp', 'cpp');

    coordinator.handleDidSave(doc1);
    coordinator.handleDidSave(doc2);

    assert.equal(coordinator.activeTimerCount, 2);

    await new Promise((resolve) => setTimeout(resolve, 40));
    assert.equal(analyzedFiles.length, 2);
    assert.ok(analyzedFiles.includes('/workspace/file1.cpp'));
    assert.ok(analyzedFiles.includes('/workspace/file2.cpp'));
    assert.equal(coordinator.activeTimerCount, 0);

    coordinator.dispose();
  });

  it('cancels pending timers on dispose', async () => {
    let called = false;
    const coordinator = new SaveAnalysisCoordinator({
      ...makeMockDeps(),
      isEnabled: () => true,
      debounceMs: 50,
      analyzeFn: () => {
        called = true;
        return Promise.resolve();
      },
    });

    const doc = makeMockDoc('/workspace/solution.cpp', 'cpp');
    coordinator.handleDidSave(doc);
    assert.equal(coordinator.activeTimerCount, 1);

    coordinator.dispose();
    assert.equal(coordinator.activeTimerCount, 0);

    await new Promise((resolve) => setTimeout(resolve, 70));
    assert.equal(called, false);
  });
});
