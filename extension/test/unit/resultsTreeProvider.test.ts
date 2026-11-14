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
import { ResultsSnapshot, ResultsTreeProvider } from '../../src/providers/resultsTreeProvider';
import { MockUri } from '../mocks/mockVscode';

function makeSampleFinding(): Finding {
  return {
    id: 'F1',
    category: 'loop_nest',
    pattern: 'nested for-loop',
    description: 'Nested loop over n and m',
    function: 'solve',
    location: { file: 'main.cpp', line: 15, column: 3 },
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
  };
}

function makeSampleResults(): AnalyzeResults {
  const f1 = makeSampleFinding();
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
      time: { status: 'likely_safe', reason: 'Within 1s' },
      memory: { status: 'likely_safe', reason: 'Within limit' },
    },
    summary: {
      finding_count: 1,
      max_time_workload: 1000000,
      dominant_finding_id: 'F1',
      coverage: 'partial',
    },
  };
}

describe('ResultsTreeProvider - Tree Structure and Accessibility (BL-508)', () => {
  it('builds tree and populates accessibilityInformation for sections, findings, and details', () => {
    const provider = new ResultsTreeProvider();
    const uri = MockUri.file('/workspace/main.cpp');
    const snapshot: ResultsSnapshot = {
      uri: uri as unknown as vscode.Uri,
      results: makeSampleResults(),
      engineVersion: '0.1.0',
    };

    provider.show(snapshot);

    const rootNodes = provider.getChildren();
    assert.ok(rootNodes.length > 0);

    // Section item
    const sectionNode = rootNodes[0]!;
    assert.equal(sectionNode.kind, 'section');
    const sectionItem = provider.getTreeItem(sectionNode);
    assert.ok(sectionItem.accessibilityInformation !== undefined);
    assert.ok(sectionItem.accessibilityInformation.label.startsWith('Section:'));

    // Section children -> finding
    const sectionChildren = provider.getChildren(sectionNode);
    assert.ok(sectionChildren.length > 0);
    const findingNode = sectionChildren.find((n) => n.kind === 'finding');
    assert.ok(findingNode !== undefined);

    const findingItem = provider.getTreeItem(findingNode);
    assert.ok(findingItem.accessibilityInformation !== undefined);
    assert.ok(findingItem.accessibilityInformation.label.includes('F1'));
    assert.ok(findingItem.accessibilityInformation.label.includes('solve'));
    assert.ok(findingItem.accessibilityInformation.label.includes('line 15'));
    assert.ok(findingItem.accessibilityInformation.label.includes('bound status proven'));
    assert.ok(findingItem.accessibilityInformation.label.includes('confidence high'));

    // Finding children -> details
    const details = provider.getChildren(findingNode);
    assert.ok(details.length > 0);
    const detailNode = details[0]!;
    assert.equal(detailNode.kind, 'detail');

    const detailItem = provider.getTreeItem(detailNode);
    assert.ok(detailItem.accessibilityInformation !== undefined);
    assert.ok(detailItem.accessibilityInformation.label.includes(detailNode.label));

    provider.dispose();
  });

  it('handles empty snapshot cleanly', () => {
    const provider = new ResultsTreeProvider();
    provider.show(undefined);
    assert.deepEqual(provider.getChildren(), []);
    provider.dispose();
  });
});
