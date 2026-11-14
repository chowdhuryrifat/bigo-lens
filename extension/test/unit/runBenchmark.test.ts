import * as assert from 'node:assert/strict';
import Module from 'node:module';
import { describe, it, beforeEach } from 'node:test';

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

import {
  getPlatformIsolationNotice,
  runBenchmark,
  type BenchmarkDependencies,
} from '../../src/commands/runBenchmark';
import type { BenchmarkResults, EngineRequest, EngineResponse } from '../../src/models/protocol';
import { DiagnosticsPublisher } from '../../src/providers/diagnosticsPublisher';
import { ResultsTreeProvider } from '../../src/providers/resultsTreeProvider';
import { EngineClient, EngineError } from '../../src/services/engineClient';
import { SchemaValidator } from '../../src/services/schemaValidator';
import { commands, Uri, window, workspace } from '../mocks/mockVscode';

class MockOutputChannel {
  public lines: string[] = [];
  public appendLine(line: string): void {
    this.lines.push(line);
  }
  public show(): void {}
  public clear(): void {
    this.lines = [];
  }
}

describe('runBenchmark - Isolation Notice & Platform Labeling (BL-507)', () => {
  it('returns appropriate transparent isolation notice for each platform', () => {
    const linuxNotice = getPlatformIsolationNotice('linux');
    assert.match(linuxNotice, /Linux namespaces/i);
    assert.match(linuxNotice, /not sandboxed/i);

    const macNotice = getPlatformIsolationNotice('darwin');
    assert.match(macNotice, /Not sandboxed/i);
    assert.match(macNotice, /sandbox-exec is deprecated/i);

    const winNotice = getPlatformIsolationNotice('win32');
    assert.match(winNotice, /Job Objects/i);
    assert.match(winNotice, /not fully sandboxed/i);

    const otherNotice = getPlatformIsolationNotice('aix');
    assert.match(otherNotice, /Not sandboxed/i);
  });
});

describe('runBenchmark - Workspace Trust & Security Enforcement (BL-507)', () => {
  let output: MockOutputChannel;
  let tree: ResultsTreeProvider;
  let diagnostics: DiagnosticsPublisher;
  let deps: BenchmarkDependencies;
  let lastErrorMessage: string | undefined;
  let warningPromptShown = false;

  beforeEach(() => {
    output = new MockOutputChannel();
    tree = new ResultsTreeProvider();
    diagnostics = new DiagnosticsPublisher();
    deps = {
      extensionRoot: '/mock/ext',
      validator: new SchemaValidator(),
      output: output as unknown as import('vscode').OutputChannel,
      tree,
      diagnostics,
    };
    lastErrorMessage = undefined;
    warningPromptShown = false;

    window.showErrorMessage = ((msg: string) => {
      lastErrorMessage = msg;
      return Promise.resolve(undefined);
    }) as unknown as typeof window.showErrorMessage;

    window.showWarningMessage = () => {
      warningPromptShown = true;
      return Promise.resolve(undefined);
    };
  });

  it('strictly refuses benchmarking when workspace is untrusted', async () => {
    workspace.isTrusted = false;
    window.activeTextEditor = {
      document: {
        uri: Uri.file('/workspace/test.cpp'),
        languageId: 'cpp',
        fileName: '/workspace/test.cpp',
        getText: () => 'int main() {}',
      },
    };

    await runBenchmark(deps);

    assert.equal(lastErrorMessage, 'BigO Lens: benchmarking is disabled in untrusted workspaces.');
    assert.equal(warningPromptShown, false, 'Must not prompt user or spawn engine if untrusted');
    workspace.isTrusted = true;
  });

  it('rejects benchmarking when no active editor is present', async () => {
    workspace.isTrusted = true;
    window.activeTextEditor = undefined;
    let warningMsg: string | undefined;
    window.showWarningMessage = ((msg: string) => {
      warningMsg = msg;
      return Promise.resolve(undefined);
    }) as unknown as typeof window.showWarningMessage;

    await runBenchmark(deps);

    assert.match(warningMsg ?? '', /open a supported file/i);
  });

  it('rejects benchmarking for unsupported language', async () => {
    workspace.isTrusted = true;
    window.activeTextEditor = {
      document: {
        uri: Uri.file('/workspace/test.txt'),
        languageId: 'plaintext',
        fileName: '/workspace/test.txt',
        getText: () => 'hello',
      },
    };
    let warningMsg: string | undefined;
    window.showWarningMessage = ((msg: string) => {
      warningMsg = msg;
      return Promise.resolve(undefined);
    }) as unknown as typeof window.showWarningMessage;

    await runBenchmark(deps);

    assert.match(warningMsg ?? '', /open a supported file/i);
  });
});

describe('runBenchmark - User Confirmation Flow & Engine Client Interactions (BL-507)', () => {
  let output: MockOutputChannel;
  let tree: ResultsTreeProvider;
  let diagnostics: DiagnosticsPublisher;
  let deps: BenchmarkDependencies;
  let promptText = '';
  let promptOptions: unknown;

  beforeEach(() => {
    workspace.isTrusted = true;
    output = new MockOutputChannel();
    tree = new ResultsTreeProvider();
    diagnostics = new DiagnosticsPublisher();
    deps = {
      extensionRoot: '/mock/ext',
      validator: new SchemaValidator(),
      output: output as unknown as import('vscode').OutputChannel,
      tree,
      diagnostics,
    };
    promptText = '';
    promptOptions = undefined;

    window.activeTextEditor = {
      document: {
        uri: Uri.file('/workspace/solution.cpp'),
        languageId: 'cpp',
        fileName: '/workspace/solution.cpp',
        getText: () => 'int main() { for(int i=0; i<100; ++i); }',
      },
    };
  });

  it('cancels without execution when modal confirmation is dismissed or denied', async () => {
    window.showWarningMessage = ((msg: string, opts: unknown) => {
      promptText = msg;
      promptOptions = opts;
      return Promise.resolve(undefined); // user clicked cancel or closed modal
    }) as unknown as typeof window.showWarningMessage;

    await runBenchmark(deps);

    assert.match(promptText, /Benchmarking compiles and executes code/);
    assert.match(promptText, /Isolation level:/);
    assert.deepEqual(promptOptions, { modal: true });
    assert.ok(output.lines.some((line) => line.includes('benchmark cancelled by user')));
  });

  it('handles engine not_implemented error cleanly with information dialog', async () => {
    window.showWarningMessage = () => Promise.resolve('Run Benchmark');

    let infoMsg = '';
    window.showInformationMessage = ((msg: string) => {
      infoMsg = msg;
      return Promise.resolve(undefined);
    }) as unknown as typeof window.showInformationMessage;

    const notImplementedResp: EngineResponse = {
      schema_version: '1.0',
      request_id: 'test-req',
      status: 'error',
      engine: { name: 'bigolens-engine', version: '0.1.0', protocol_version: '1.0', frontend: 'clang' },
      results: null,
      diagnostics: [],
      errors: [
        {
          code: 'not_implemented',
          message: 'dynamic benchmarking is planned for milestone M4 and is not implemented',
        },
      ],
    };

    let sentRequest: EngineRequest | undefined;
    const fakeClient = {
      send: (req: EngineRequest): Promise<EngineResponse> => {
        sentRequest = req;
        return Promise.resolve(notImplementedResp);
      },
    } as unknown as EngineClient;

    await runBenchmark({ ...deps, client: fakeClient });

    assert.ok(sentRequest);
    assert.equal(sentRequest.action, 'benchmark');
    assert.match(infoMsg, /dynamic benchmarking is planned for milestone M4 and is not implemented/);
    assert.ok(output.lines.some((l) => l.includes('not implemented')));
  });

  it('handles general engine error responses with error dialog', async () => {
    window.showWarningMessage = () => Promise.resolve('Run Benchmark');

    let errorMsg = '';
    window.showErrorMessage = ((msg: string) => {
      errorMsg = msg;
      return Promise.resolve(undefined);
    }) as unknown as typeof window.showErrorMessage;

    const failureResp: EngineResponse = {
      schema_version: '1.0',
      request_id: 'test-req',
      status: 'error',
      engine: { name: 'bigolens-engine', version: '0.1.0', protocol_version: '1.0', frontend: 'clang' },
      results: null,
      diagnostics: [],
      errors: [
        {
          code: 'compilation_failed',
          message: 'undefined reference to main',
        },
      ],
    };

    const fakeClient = {
      send: (): Promise<EngineResponse> => {
        return Promise.resolve(failureResp);
      },
    } as unknown as EngineClient;

    await runBenchmark({ ...deps, client: fakeClient });

    assert.match(errorMsg, /compilation_failed: undefined reference to main/);
    assert.ok(output.lines.some((l) => l.includes('Benchmark failed:')));
  });

  it('handles cancellation properly by logging and notifying user', async () => {
    window.showWarningMessage = () => Promise.resolve('Run Benchmark');

    let infoMsg = '';
    window.showInformationMessage = ((msg: string) => {
      infoMsg = msg;
      return Promise.resolve(undefined);
    }) as unknown as typeof window.showInformationMessage;

    const fakeClient = {
      send: (): Promise<EngineResponse> => {
        return Promise.reject(new EngineError('analysis cancelled', 'cancelled'));
      },
    } as unknown as EngineClient;

    await runBenchmark({ ...deps, client: fakeClient });

    assert.match(infoMsg, /benchmark cancelled/);
    assert.ok(output.lines.some((l) => l.includes('benchmark cancelled by user')));
  });

  it('processes benchmark results and triggers openReport on success', async () => {
    window.showWarningMessage = () => Promise.resolve('Run Benchmark');

    const mockBenchmarkResults: BenchmarkResults = {
      findings: [],
      memory: {
        static_storage_bytes: 0,
        static_storage_saturated: false,
        largest_local_array_bytes: 0,
        estimated_minimum_bytes: null,
        estimated_minimum_status: 'proven',
        assumptions: [],
        items: [],
      },
      feasibility: {
        time: { status: 'likely_safe', reason: 'Fits within 1000 ms limit' },
        memory: { status: 'likely_safe', reason: 'Fits within 256 MB' },
      },
      summary: {
        finding_count: 0,
        max_time_workload: 5000,
        dominant_finding_id: null,
        coverage: 'partial',
      },
      benchmark: {
        isolated: false,
        isolation_level: 'rlimits only',
        samples: [
          {
            size: { n: 1000 },
            wall_time_median_ms: 0.25,
            wall_time_mad_ms: 0.02,
            peak_rss_bytes: 1024 * 1024 * 4,
            raw_samples_ms: [0.24, 0.25, 0.26],
          },
          {
            size: { n: 2000 },
            wall_time_median_ms: 0.51,
            wall_time_mad_ms: 0.03,
            peak_rss_bytes: 1024 * 1024 * 4,
            raw_samples_ms: [0.5, 0.51, 0.53],
          },
        ],
        empirical_fit: {
          model: 'linear',
          r_squared: 0.998,
          rss: 0.0001,
          slope: 0.00025,
          intercept: 0.005,
          ambiguous: false,
        },
        extrapolated_runtime: {
          target_size: { n: 100000 },
          min_ms: 24.5,
          central_ms: 25.0,
          max_ms: 26.2,
          extrapolated: true,
        },
        warnings: ['extrapolated beyond measured points'],
      },
    };

    const successResp: EngineResponse = {
      schema_version: '1.0',
      request_id: 'test-req',
      status: 'ok',
      engine: { name: 'bigolens-engine', version: '0.1.0', protocol_version: '1.0', frontend: 'clang' },
      results: mockBenchmarkResults,
      diagnostics: [],
      errors: [],
    };

    let executedCommand = '';
    commands.executeCommand = ((cmd: string) => {
      executedCommand = cmd;
      return Promise.resolve(undefined);
    }) as unknown as typeof commands.executeCommand;

    let snapshotReceived = false;
    const testDeps: BenchmarkDependencies = {
      ...deps,
      client: {
        send: (): Promise<EngineResponse> => Promise.resolve(successResp),
      } as unknown as EngineClient,
      onResults: (snapshot) => {
        snapshotReceived = true;
        assert.equal(snapshot.results.benchmark?.empirical_fit.model, 'linear');
      },
    };

    await runBenchmark(testDeps);

    assert.equal(snapshotReceived, true);
    assert.equal(executedCommand, 'bigoLens.openReport');
  });
});
