import assert from 'node:assert/strict';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import Module from 'node:module';
import * as os from 'node:os';
import * as path from 'node:path';
import { afterEach, beforeEach, describe, it } from 'node:test';

import * as mockVscode from '../mocks/mockVscode';

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

// Now import panels and services after installing the module hook
import { extractDetectedVariables } from '../../src/commands/configureConstraints';
import { ConstraintFormPanel } from '../../src/panels/constraintFormPanel';
import type { ConstraintFormState } from '../../src/services/constraintsFormModel';
import type { ReportSnapshot } from '../../src/services/reportHtml';
import { SchemaValidator } from '../../src/services/schemaValidator';

function sampleSnapshot(): ReportSnapshot {
  return {
    displayPath: 'src/solution.cpp',
    engineVersion: '0.1.0',
    results: {
      summary: {
        finding_count: 2,
        coverage: 'partial',
        dominant_finding_id: 'F1',
        max_time_workload: 200000,
      },
      findings: [
        {
          id: 'F1',
          category: 'loop_nest',
          pattern: 'counted-for-loop',
          function: 'solve',
          location: { file: 'src/solution.cpp', line: 15, column: 5 },
          description: 'outer loop',
          time: {
            expression: 'n',
            asymptotic: 'O(n)',
            variables: ['n', 't'],
            workload: {
              status: 'proven',
              upper_bound: 200000,
              method: 'sum-constraint-packing',
              assumptions: [],
              missing_bounds: [],
            },
          },
          space: null,
          bound_status: 'proven',
          confidence: 'high',
          assumptions: [],
          limitations: [],
          provenance: 'clang-loop-analyzer',
        },
        {
          id: 'F2',
          category: 'loop_nest',
          pattern: 'counted-for-loop',
          function: 'helper',
          location: { file: 'src/solution.cpp', line: 30, column: 5 },
          description: 'inner loop',
          time: {
            expression: 'm',
            asymptotic: 'O(m)',
            variables: ['m'],
            workload: {
              status: 'unknown',
              upper_bound: null,
              method: null,
              assumptions: [],
              missing_bounds: ['m'],
            },
          },
          space: null,
          bound_status: 'unknown',
          confidence: 'none',
          assumptions: [],
          limitations: [],
          provenance: 'clang-loop-analyzer',
        },
      ],
      memory: {
        static_storage_bytes: 0,
        static_storage_saturated: false,
        largest_local_array_bytes: 0,
        estimated_minimum_bytes: 0,
        estimated_minimum_status: 'proven',
        assumptions: [],
        items: [],
      },
      feasibility: {
        time: { status: 'likely_safe', reason: 'ok' },
        memory: { status: 'likely_safe', reason: 'ok' },
      },
    },
  };
}

describe('extractDetectedVariables', () => {
  it('extracts variables and flags missing_bounds correctly', () => {
    const snapshot = sampleSnapshot();
    const detected = extractDetectedVariables(snapshot, '/workspace/src/solution.cpp');
    assert.equal(detected.length, 3);

    const nVar = detected.find((v) => v.name === 'n');
    assert.ok(nVar);
    assert.equal(nVar.status, 'used');
    assert.equal(nVar.locationHint, 'solve (line 15)');

    const mVar = detected.find((v) => v.name === 'm');
    assert.ok(mVar);
    assert.equal(mVar.status, 'missing_bound');
    assert.equal(mVar.locationHint, 'helper (line 30)');
  });

  it('returns empty array when snapshot is undefined or does not match file', () => {
    assert.deepEqual(extractDetectedVariables(undefined, 'src/solution.cpp'), []);
    assert.deepEqual(extractDetectedVariables(sampleSnapshot(), '/workspace/other/unrelated.cpp'), []);
  });
});

describe('ConstraintFormPanel lifecycle, security and messaging', () => {
  const extensionUri = mockVscode.MockUri.file('/path/to/extension') as unknown as import('vscode').Uri;
  const validator = new SchemaValidator();
  let tempDir: string;
  let sidecarPath: string;

  beforeEach(async () => {
    if (ConstraintFormPanel.currentPanel) {
      ConstraintFormPanel.currentPanel.dispose();
    }
    mockVscode.resetCreatedPanels();
    tempDir = await mkdtemp(path.join(os.tmpdir(), 'bigolens-test-form-'));
    sidecarPath = path.join(tempDir, 'solution.cpp.bigolens.json');
  });

  afterEach(async () => {
    if (ConstraintFormPanel.currentPanel) {
      ConstraintFormPanel.currentPanel.dispose();
    }
    await rm(tempDir, { recursive: true, force: true });
  });

  it('creates webview panel with strictly restricted localResourceRoots and enableScripts', () => {
    const initialState: ConstraintFormState = {
      variables: [{ name: 'n', min: '1', max: '200000' }],
      sumConstraints: [],
      productConstraints: [],
      linearConstraints: [],
    };

    const panelInstance = ConstraintFormPanel.createOrShow({
      extensionUri,
      displayPath: 'src/solution.cpp',
      sidecarPath,
      initialState,
      validator,
    });

    assert.ok(panelInstance);
    assert.equal(mockVscode.createdPanels.length, 1);
    const created = mockVscode.createdPanels[0];
    assert.ok(created);
    assert.equal(created.options.enableScripts, true);
    assert.equal(created.options.localResourceRoots?.length, 1);
    const roots = created.options.localResourceRoots;
    assert.ok(roots);
    const firstRoot = roots[0];
    assert.ok(firstRoot);
    assert.equal(firstRoot.fsPath, '/path/to/extension/media');
  });

  it('renders HTML with strict CSP and title', () => {
    const initialState: ConstraintFormState = {
      variables: [{ name: 'n', min: '1', max: '200000' }],
      sumConstraints: [],
      productConstraints: [],
      linearConstraints: [],
    };

    ConstraintFormPanel.createOrShow({
      extensionUri,
      displayPath: 'src/solution.cpp',
      sidecarPath,
      initialState,
      validator,
    });

    const panel = mockVscode.createdPanels[0];
    assert.ok(panel);
    assert.equal(panel.title, 'Constraints: solution.cpp');
    assert.ok(panel.webview.html.includes("default-src 'none'"));
  });

  it('handles validate message: success for valid state', async () => {
    const initialState: ConstraintFormState = {
      variables: [{ name: 'n', min: '1', max: '200000' }],
      sumConstraints: [],
      productConstraints: [],
      linearConstraints: [],
    };

    const formPanel = ConstraintFormPanel.createOrShow({
      extensionUri,
      displayPath: 'src/solution.cpp',
      sidecarPath,
      initialState,
      validator,
    });

    const panel = mockVscode.createdPanels[0];
    assert.ok(panel);

    await formPanel.handleMessage({
      type: 'validate',
      state: initialState,
    });

    assert.equal(panel.webview.postedMessages.length, 1);
    const reply = panel.webview.postedMessages[0] as { type: string; ok: boolean };
    assert.equal(reply.type, 'validationResult');
    assert.equal(reply.ok, true);
  });

  it('handles validate message: returns errors for invalid state (e.g. min > max)', async () => {
    const invalidState: ConstraintFormState = {
      variables: [{ name: 'n', min: '500', max: '100' }],
      sumConstraints: [],
      productConstraints: [],
      linearConstraints: [],
    };

    const formPanel = ConstraintFormPanel.createOrShow({
      extensionUri,
      displayPath: 'src/solution.cpp',
      sidecarPath,
      initialState: invalidState,
      validator,
    });

    const panel = mockVscode.createdPanels[0];
    assert.ok(panel);

    await formPanel.handleMessage({
      type: 'validate',
      state: invalidState,
    });

    assert.equal(panel.webview.postedMessages.length, 1);
    const reply = panel.webview.postedMessages[0] as {
      type: string;
      ok: boolean;
      errors?: string[];
    };
    assert.equal(reply.type, 'validationResult');
    assert.equal(reply.ok, false);
    assert.ok(reply.errors && reply.errors.length > 0);
  });

  it('handles save message: writes to sidecar file and executes onAnalyze when andAnalyze is true', async () => {
    let analyzed = false;
    const validState: ConstraintFormState = {
      variables: [{ name: 'n', min: '1', max: '1000' }],
      sumConstraints: [],
      productConstraints: [],
      linearConstraints: [],
      timeLimitMs: '1500',
    };

    const formPanel = ConstraintFormPanel.createOrShow({
      extensionUri,
      displayPath: 'src/solution.cpp',
      sidecarPath,
      initialState: validState,
      validator,
      onAnalyze: () => {
        analyzed = true;
      },
    });

    const panel = mockVscode.createdPanels[0];
    assert.ok(panel);

    await formPanel.handleMessage({
      type: 'save',
      state: validState,
      andAnalyze: true,
    });

    assert.equal(panel.webview.postedMessages.length, 1);
    const reply = panel.webview.postedMessages[0] as {
      type: string;
      ok: boolean;
      andAnalyze?: boolean;
    };
    assert.equal(reply.type, 'saveResult');
    assert.equal(reply.ok, true);
    assert.equal(reply.andAnalyze, true);
    assert.equal(analyzed, true);

    // Verify sidecar was written to disk
    const content = await readFile(sidecarPath, 'utf8');
    const parsed = JSON.parse(content) as {
      variables: { n: { min: number; max: number } };
      time_limit_ms: number;
    };
    assert.equal(parsed.variables.n.max, 1000);
    assert.equal(parsed.time_limit_ms, 1500);
  });
});
