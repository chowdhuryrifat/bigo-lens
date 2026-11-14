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

import type { AnalyzeResults, Finding } from '../../src/models/protocol';
import {
  formatFeasibilityIndicator,
  formatStatusBarText,
  formatStatusBarTooltip,
  getDominantFinding,
  isCppDocument,
  OPEN_REPORT_COMMAND,
  StatusBarManager,
} from '../../src/services/statusBar';
import { MockStatusBarItem, MockUri } from '../mocks/mockVscode';

function makeFinding(overrides: Partial<Finding> = {}): Finding {
  return {
    id: 'f1',
    category: 'loop_nest',
    pattern: 'nested for-loop',
    description: 'Nested loop over n and m',
    function: 'solve',
    location: { file: 'main.cpp', line: 10, column: 5 },
    time: {
      expression: 'n * m',
      asymptotic: 'O(n * m)',
      variables: ['n', 'm'],
      workload: {
        status: 'proven',
        upper_bound: 1000000,
        method: 'exact',
        assumptions: [],
        missing_bounds: [],
      },
    },
    space: null,
    bound_status: 'proven',
    confidence: 'high',
    assumptions: [],
    limitations: [],
    provenance: 'static_analysis',
    ...overrides,
  };
}

function makeResults(overrides: Partial<AnalyzeResults> = {}): AnalyzeResults {
  const f1 = makeFinding();
  return {
    findings: [f1],
    memory: {
      static_storage_bytes: 4096,
      static_storage_saturated: false,
      largest_local_array_bytes: 1024,
      estimated_minimum_bytes: 8192,
      estimated_minimum_status: 'proven',
      assumptions: [],
      items: [],
    },
    feasibility: {
      time: { status: 'likely_safe', reason: 'Workload 1e6 is within time limit budget' },
      memory: { status: 'likely_safe', reason: 'Memory 8 KiB is within limit' },
    },
    summary: {
      finding_count: 1,
      max_time_workload: 1000000,
      dominant_finding_id: 'f1',
      coverage: 'partial',
    },
    ...overrides,
  };
}

describe('StatusBar text and indicators', () => {
  it('formats feasibility indicators correctly', () => {
    assert.equal(formatFeasibilityIndicator('likely_safe'), '✓');
    assert.equal(formatFeasibilityIndicator('borderline'), '⚠');
    assert.equal(formatFeasibilityIndicator('high_risk'), '✕');
    assert.equal(formatFeasibilityIndicator('unknown'), '?');
  });

  it('selects the dominant finding by dominant_finding_id', () => {
    const f1 = makeFinding({
      id: 'f1',
      time: {
        expression: 'n',
        asymptotic: 'O(n)',
        variables: ['n'],
        workload: {
          status: 'proven',
          upper_bound: 100,
          method: 'exact',
          assumptions: [],
          missing_bounds: [],
        },
      },
    });
    const f2 = makeFinding({
      id: 'f2',
      time: {
        expression: 'n^2',
        asymptotic: 'O(n^2)',
        variables: ['n'],
        workload: {
          status: 'proven',
          upper_bound: 10000,
          method: 'exact',
          assumptions: [],
          missing_bounds: [],
        },
      },
    });
    const results = makeResults({
      findings: [f1, f2],
      summary: { finding_count: 2, max_time_workload: 10000, dominant_finding_id: 'f2', coverage: 'partial' },
    });
    const dominant = getDominantFinding(results);
    assert.equal(dominant?.id, 'f2');
  });

  it('falls back to findings[0] when dominant_finding_id is null or missing', () => {
    const f1 = makeFinding({ id: 'first' });
    const results = makeResults({
      findings: [f1],
      summary: { finding_count: 1, max_time_workload: null, dominant_finding_id: null, coverage: 'partial' },
    });
    assert.equal(getDominantFinding(results)?.id, 'first');
  });

  it('formats status bar text with asymptotic, workload ops, and feasibility', () => {
    const results = makeResults();
    const text = formatStatusBarText(results);
    assert.equal(text, 'BigO: O(n * m) (≤ 1.000e6) · time ✓ · mem ✓');
  });

  it('formats status bar text without workload when max_time_workload is null', () => {
    const results = makeResults({
      summary: { finding_count: 1, max_time_workload: null, dominant_finding_id: 'f1', coverage: 'partial' },
      feasibility: {
        time: { status: 'unknown', reason: 'No time limit or workload unknown' },
        memory: { status: 'borderline', reason: 'High static allocation' },
      },
    });
    const text = formatStatusBarText(results);
    assert.equal(text, 'BigO: O(n * m) · time ? · mem ⚠');
  });

  it('formats status bar text when findings list is empty', () => {
    const results = makeResults({
      findings: [],
      summary: { finding_count: 0, max_time_workload: null, dominant_finding_id: null, coverage: 'partial' },
    });
    const text = formatStatusBarText(results);
    assert.equal(text, 'BigO: none · time ✓ · mem ✓');
  });

  it('formats markdown tooltip with complete summary', () => {
    const results = makeResults();
    const tooltip = formatStatusBarTooltip(results, 'src/solution.cpp');
    const md = tooltip.value;
    assert.ok(md.includes('src/solution.cpp'));
    assert.ok(md.includes('O(n * m)'));
    assert.ok(md.includes('≤ 1.000e6 ops'));
    assert.ok(md.includes('likely_safe'));
    assert.ok(md.includes(OPEN_REPORT_COMMAND));
  });

  it('recognizes C++ documents and rejects non-C++ files', () => {
    assert.equal(isCppDocument({ languageId: 'cpp' } as vscode.TextDocument), true);
    assert.equal(isCppDocument({ languageId: 'c' } as vscode.TextDocument), true);
    assert.equal(isCppDocument({ languageId: 'python' } as vscode.TextDocument), false);
    assert.equal(isCppDocument({ languageId: 'java' } as vscode.TextDocument), false);
    assert.equal(isCppDocument({ languageId: 'markdown' } as vscode.TextDocument), false);
  });
});

describe('StatusBarManager lifecycle and visibility', () => {
  it('initializes with right alignment and hides initially', () => {
    const mockItem = new MockStatusBarItem(2, 100);
    const manager = new StatusBarManager(mockItem as unknown as vscode.StatusBarItem);
    assert.equal(mockItem.command, OPEN_REPORT_COMMAND);
    assert.equal(mockItem.isVisible, false);
    manager.dispose();
    assert.equal(mockItem.isDisposed, true);
  });

  it('hides for non-C++ editors even if analysis results exist', () => {
    const mockItem = new MockStatusBarItem();
    const manager = new StatusBarManager(mockItem as unknown as vscode.StatusBarItem);
    const uri = MockUri.file('/workspace/script.py');
    manager.updateResults(uri as unknown as vscode.Uri, makeResults(), 'script.py');

    const pythonEditor = {
      document: { uri, languageId: 'python' } as unknown as vscode.TextDocument,
    } as vscode.TextEditor;

    manager.onActiveEditorChange(pythonEditor);
    assert.equal(mockItem.isVisible, false);
  });

  it('shows and updates for C++ editor after analysis', () => {
    const mockItem = new MockStatusBarItem();
    const manager = new StatusBarManager(mockItem as unknown as vscode.StatusBarItem);
    const uri = MockUri.file('/workspace/solution.cpp');
    const results = makeResults();

    const cppEditor = {
      document: { uri, languageId: 'cpp' } as unknown as vscode.TextDocument,
    } as vscode.TextEditor;

    // First switch before analysis -> hidden
    manager.onActiveEditorChange(cppEditor);
    assert.equal(mockItem.isVisible, false);

    // Now update results for this document
    manager.updateResults(uri as unknown as vscode.Uri, results, 'solution.cpp');
    manager.refresh(cppEditor);

    assert.equal(mockItem.isVisible, true);
    assert.ok(mockItem.text.includes('BigO: O(n * m)'));
    assert.ok(mockItem.text.includes('$(graph)'));
  });

  it('hides when switching from analyzed C++ to non-C++ editor', () => {
    const mockItem = new MockStatusBarItem();
    const manager = new StatusBarManager(mockItem as unknown as vscode.StatusBarItem);
    const cppUri = MockUri.file('/workspace/solution.cpp');
    const pyUri = MockUri.file('/workspace/script.py');

    const cppEditor = {
      document: { uri: cppUri, languageId: 'cpp' } as unknown as vscode.TextDocument,
    } as vscode.TextEditor;
    const pyEditor = {
      document: { uri: pyUri, languageId: 'python' } as unknown as vscode.TextDocument,
    } as vscode.TextEditor;

    manager.updateResults(cppUri as unknown as vscode.Uri, makeResults(), 'solution.cpp');
    manager.onActiveEditorChange(cppEditor);
    assert.equal(mockItem.isVisible, true);

    manager.onActiveEditorChange(pyEditor);
    assert.equal(mockItem.isVisible, false);

    manager.onActiveEditorChange(undefined);
    assert.equal(mockItem.isVisible, false);
  });

  it('clears state on clear()', () => {
    const mockItem = new MockStatusBarItem();
    const manager = new StatusBarManager(mockItem as unknown as vscode.StatusBarItem);
    const cppUri = MockUri.file('/workspace/solution.cpp');
    const cppEditor = {
      document: { uri: cppUri, languageId: 'cpp' } as unknown as vscode.TextDocument,
    } as vscode.TextEditor;

    manager.updateResults(cppUri as unknown as vscode.Uri, makeResults(), 'solution.cpp');
    manager.onActiveEditorChange(cppEditor);
    assert.equal(mockItem.isVisible, true);

    manager.clear();
    assert.equal(mockItem.isVisible, false);
  });

  it('sets screen-reader accessibilityInformation on status bar item (BL-508)', () => {
    const mockItem = new MockStatusBarItem();
    const manager = new StatusBarManager(mockItem as unknown as vscode.StatusBarItem);
    const cppUri = MockUri.file('/workspace/solution.cpp');
    const cppEditor = {
      document: { uri: cppUri, languageId: 'cpp' } as unknown as vscode.TextDocument,
    } as vscode.TextEditor;

    manager.updateResults(cppUri as unknown as vscode.Uri, makeResults(), 'solution.cpp');
    manager.onActiveEditorChange(cppEditor);

    assert.equal(mockItem.isVisible, true);
    assert.ok(mockItem.accessibilityInformation !== undefined);
    assert.ok(mockItem.accessibilityInformation.label.startsWith('BigO Lens Analysis Summary:'));
    assert.ok(mockItem.accessibilityInformation.label.includes('O(n * m)'));
  });
});
