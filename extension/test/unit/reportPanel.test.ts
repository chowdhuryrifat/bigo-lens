import assert from 'node:assert/strict';
import Module from 'node:module';
import { beforeEach, describe, it } from 'node:test';

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

// Now import ReportPanel after installing the module hook
import { ReportPanel } from '../../src/panels/reportPanel';
import type { ReportSnapshot } from '../../src/services/reportHtml';

function sampleSnapshot(): ReportSnapshot {
  return {
    displayPath: 'src/solution.cpp',
    engineVersion: '0.1.0',
    results: {
      summary: {
        finding_count: 1,
        coverage: 'partial',
        dominant_finding_id: 'F1',
        max_time_workload: 5000,
      },
      findings: [
        {
          id: 'F1',
          category: 'loop_nest',
          pattern: 'counted-for-loop',
          function: 'compute',
          location: { file: 'src/solution.cpp', line: 25, column: 3 },
          description: 'iterates over range',
          time: {
            expression: 'n',
            asymptotic: 'O(n)',
            variables: ['n'],
            workload: {
              status: 'proven',
              upper_bound: 5000,
              method: 'maxima-evaluation',
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
      ],
      memory: {
        static_storage_bytes: 8,
        static_storage_saturated: false,
        largest_local_array_bytes: 0,
        estimated_minimum_bytes: 8,
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

describe('ReportPanel lifecycle, security and messaging', () => {
  const extensionUri = mockVscode.MockUri.file('/path/to/extension') as unknown as import('vscode').Uri;

  beforeEach(() => {
    if (ReportPanel.currentPanel) {
      ReportPanel.currentPanel.dispose();
    }
    mockVscode.resetCreatedPanels();
    mockVscode.navigationRecords.length = 0;
  });

  it('creates webview panel with strictly restricted localResourceRoots and enableScripts', () => {
    const panelInstance = ReportPanel.createOrShow(extensionUri);

    assert.equal(ReportPanel.currentPanel, panelInstance);
    assert.equal(mockVscode.createdPanels.length, 1);

    const mockPanel = mockVscode.createdPanels[0]!;
    assert.equal(mockPanel.viewType, 'bigoLens.report');
    assert.equal(mockPanel.options.enableScripts, true);
    assert.equal(mockPanel.options.retainContextWhenHidden, true);

    const roots = mockPanel.options.localResourceRoots;
    assert.ok(roots, 'localResourceRoots must be defined');
    assert.equal(roots.length, 1, 'localResourceRoots must have exactly 1 entry');

    const mediaRoot = roots[0]!.fsPath;
    assert.ok(mediaRoot.endsWith('/media'), `localResourceRoots must end with /media, got: ${mediaRoot}`);
  });

  it('renders HTML with strict CSP meta tag when opened', () => {
    ReportPanel.createOrShow(extensionUri, sampleSnapshot());
    const mockPanel = mockVscode.createdPanels[0]!;

    assert.ok(mockPanel.webview.html.includes('<!DOCTYPE html>'));
    assert.ok(mockPanel.webview.html.includes('<meta http-equiv="Content-Security-Policy"'));
    assert.ok(mockPanel.webview.html.includes("default-src 'none'"));
    assert.ok(mockPanel.webview.html.includes('img-src vscode-webview:'));
    assert.ok(mockPanel.webview.html.includes('font-src vscode-webview:'));
    assert.ok(mockPanel.webview.html.includes('src/solution.cpp'));
    assert.ok(mockPanel.webview.html.includes('compute()'));
  });

  it('reveals existing panel rather than creating duplicate when called again', () => {
    const firstPanel = ReportPanel.createOrShow(extensionUri);
    assert.equal(mockVscode.createdPanels.length, 1);

    const secondPanel = ReportPanel.createOrShow(extensionUri, sampleSnapshot());
    assert.equal(firstPanel, secondPanel);
    assert.equal(mockVscode.createdPanels.length, 1, 'must not create a second panel');

    const mockPanel = mockVscode.createdPanels[0]!;
    assert.ok(mockPanel.webview.html.includes('src/solution.cpp'));
  });

  it('updates HTML when updateSnapshot is called', () => {
    const panel = ReportPanel.createOrShow(extensionUri);
    const mockPanel = mockVscode.createdPanels[0]!;
    assert.ok(mockPanel.webview.html.includes('No Analysis Available'));

    panel.updateSnapshot(sampleSnapshot());
    assert.ok(mockPanel.webview.html.includes('src/solution.cpp'));
    assert.ok(mockPanel.webview.html.includes('compute()'));
  });

  it('rejects unknown messages from postMessage without error or action', () => {
    ReportPanel.createOrShow(extensionUri, sampleSnapshot());
    const mockPanel = mockVscode.createdPanels[0]!;

    // Malicious or unknown messages should be silently ignored/rejected
    mockPanel.webview.simulateMessage({ type: 'evil_command' });
    mockPanel.webview.simulateMessage(null);
    mockPanel.webview.simulateMessage({ type: 'navigate', location: { line: -1 } });

    assert.equal(mockVscode.navigationRecords.length, 0);
  });

  it('handles valid navigate postMessage and navigates to location', async () => {
    ReportPanel.createOrShow(extensionUri, sampleSnapshot());
    const mockPanel = mockVscode.createdPanels[0]!;

    mockPanel.webview.simulateMessage({
      type: 'navigate',
      location: { file: '/path/to/solution.cpp', line: 25, column: 3 },
    });

    // Wait a tick for async navigation to complete
    await new Promise((resolve) => setTimeout(resolve, 10));

    assert.equal(mockVscode.navigationRecords.length, 1);
    const nav = mockVscode.navigationRecords[0]!;
    assert.equal(nav.documentUri.fsPath, '/path/to/solution.cpp');
    assert.equal(nav.selection.start.line, 24); // 0-based
    assert.equal(nav.selection.start.character, 2); // 0-based
  });

  it('resets currentPanel reference upon disposal', () => {
    const panel = ReportPanel.createOrShow(extensionUri);
    assert.equal(ReportPanel.currentPanel, panel);

    panel.dispose();
    assert.equal(ReportPanel.currentPanel, undefined);
  });
});
