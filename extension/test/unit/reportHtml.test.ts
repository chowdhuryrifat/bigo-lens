import assert from 'node:assert/strict';
import { describe, it } from 'node:test';

import type { AnalyzeResults, Finding } from '../../src/models/protocol';
import {
  buildCspMetaTag,
  escapeHtml,
  explainBoundStatus,
  explainConfidence,
  generateNonce,
  renderReportHtml,
  renderWorkloadChart,
  type ReportSnapshot,
} from '../../src/services/reportHtml';

function sampleResults(): AnalyzeResults {
  return {
    summary: {
      finding_count: 2,
      coverage: 'partial',
      dominant_finding_id: 'F1',
      max_time_workload: 10000,
    },
    findings: [
      {
        id: 'F1',
        category: 'loop_nest',
        pattern: 'counted-for-loop',
        function: 'solve',
        location: { file: 'main.cpp', line: 10, column: 5 },
        description: 'standard loop over n',
        time: {
          expression: 'n',
          asymptotic: 'O(n)',
          variables: ['n'],
          workload: {
            status: 'proven',
            upper_bound: 10000,
            method: 'pointwise-maxima',
            assumptions: ['n <= 10000'],
            missing_bounds: [],
          },
        },
        space: {
          expression: '4 * n',
          asymptotic: 'O(n)',
          variables: ['n'],
          workload: {
            status: 'estimated',
            upper_bound: 40000,
            method: 'linear-scaling',
            assumptions: [],
            missing_bounds: [],
          },
        },
        bound_status: 'proven',
        confidence: 'high',
        assumptions: ['n <= 10000'],
        limitations: [],
        provenance: 'clang-loop-analyzer',
      },
      {
        id: 'F2',
        category: 'recursion',
        pattern: 'collatz-recursion',
        function: 'collatz',
        location: { file: 'main.cpp', line: 25, column: 1 },
        description: 'collatz conjecture step recursion',
        time: {
          expression: 'unknown',
          asymptotic: null,
          variables: ['x'],
          workload: {
            status: 'unknown',
            upper_bound: null,
            method: null,
            assumptions: [],
            missing_bounds: ['x'],
          },
        },
        space: null,
        bound_status: 'unknown',
        confidence: 'none',
        assumptions: [],
        limitations: ['data-dependent step condition', 'open mathematical problem'],
        provenance: 'bigolens-syntax',
      },
    ],
    memory: {
      static_storage_bytes: 4,
      static_storage_saturated: false,
      largest_local_array_bytes: 1024,
      estimated_minimum_bytes: 4096,
      estimated_minimum_status: 'proven',
      assumptions: ['flat memory model'],
      items: [
        {
          name: 'buffer',
          kind: 'global_static',
          type: 'int[1000]',
          exact_bytes: 4000,
          symbolic_bytes: null,
          location: { file: 'main.cpp', line: 3, column: 1 },
        },
      ],
    },
    feasibility: {
      time: { status: 'likely_safe', reason: 'workload is well within the 1-second budget' },
      memory: { status: 'likely_safe', reason: '4 KiB is well within the 256 MiB limit' },
    },
  };
}

describe('reportHtml - CSP and Nonce', () => {
  it('generates a fresh cryptographically random base64 nonce', () => {
    const nonce1 = generateNonce();
    const nonce2 = generateNonce();

    assert.ok(nonce1.length >= 20, 'nonce should be at least 20 chars');
    assert.match(nonce1, /^[A-Za-z0-9+/=]+$/, 'nonce should be valid base64');
    assert.notEqual(nonce1, nonce2, 'two consecutive nonces must not be identical');
  });

  it('builds CSP meta tag with all required directives', () => {
    const nonce = 'TEST_NONCE_VALUE_123';
    const cspSource = 'vscode-webview:';
    const tag = buildCspMetaTag(cspSource, nonce);

    assert.ok(tag.startsWith('<meta http-equiv="Content-Security-Policy" content="'));
    assert.ok(tag.includes("default-src 'none'"));
    assert.ok(tag.includes(`script-src 'nonce-${nonce}'`));
    assert.ok(tag.includes(`style-src ${cspSource} 'nonce-${nonce}'`));
    assert.ok(tag.includes(`img-src ${cspSource}`));
    assert.ok(tag.includes(`font-src ${cspSource}`));
  });

  it('ensures each rendered HTML gets a fresh nonce if not explicitly provided', () => {
    const html1 = renderReportHtml({ cspSource: 'vscode-webview:' });
    const html2 = renderReportHtml({ cspSource: 'vscode-webview:' });

    const nonce1 = html1.match(/script-src 'nonce-([^']+)'/)?.[1];
    const nonce2 = html2.match(/script-src 'nonce-([^']+)'/)?.[1];

    assert.ok(nonce1, 'nonce1 should be present in html1');
    assert.ok(nonce2, 'nonce2 should be present in html2');
    assert.notEqual(nonce1, nonce2, 'rendered pages should receive fresh nonces');
  });

  it('uses explicitly provided nonce when supplied', () => {
    const customNonce = 'my-custom-test-nonce';
    const html = renderReportHtml({
      cspSource: 'vscode-webview:',
      nonce: customNonce,
      scriptUri: 'vscode-webview://media/report.js',
    });

    assert.ok(html.includes(`script-src 'nonce-${customNonce}'`));
    assert.ok(html.includes(`style-src vscode-webview: 'nonce-${customNonce}'`));
    assert.ok(html.includes(`<script nonce="${customNonce}" src="vscode-webview://media/report.js">`));
  });
});

describe('reportHtml - HTML Escaping and XSS Prevention', () => {
  it('escapes special HTML characters', () => {
    assert.equal(escapeHtml('Normal text'), 'Normal text');
    assert.equal(
      escapeHtml('a < b & c > d "quotes" \'single\''),
      'a &lt; b &amp; c &gt; d &quot;quotes&quot; &#39;single&#39;',
    );
  });

  it('renders injected <img onerror> in function name strictly as text', () => {
    const xssPayload = '<img onerror=alert(1)>';
    const results = sampleResults();
    results.findings[0]!.function = xssPayload;

    const snapshot: ReportSnapshot = {
      displayPath: 'src/solution.cpp',
      results,
      engineVersion: '0.1.0',
    };

    const html = renderReportHtml({
      cspSource: 'vscode-webview:',
      snapshot,
    });

    assert.ok(
      html.includes('&lt;img onerror=alert(1)&gt;'),
      'The escaped payload must be present as text in the HTML',
    );
    assert.ok(
      !html.includes('<img onerror'),
      'Raw <img onerror tag must not be present anywhere in the generated HTML',
    );
  });

  it('escapes dynamic strings in patterns, descriptions, expressions, assumptions and file paths', () => {
    const results = sampleResults();
    results.findings[0]!.pattern = '<script>evil()</script>';
    results.findings[0]!.description = '<svg onload=alert(2)>';
    if (results.findings[0]!.time) {
      results.findings[0]!.time.expression = 'n * <iframe src=javascript:alert(3)>';
      results.findings[0]!.time.asymptotic = 'O(n) <script>';
    }
    results.findings[0]!.assumptions = ['<b onmouseover=evil()>n is positive</b>'];
    results.findings[0]!.limitations = ['<a href="javascript:alert(4)">limitation</a>'];

    const snapshot: ReportSnapshot = {
      displayPath: 'test/<script>.cpp',
      results,
      engineVersion: '0.1.0" onfocus="alert(5)',
    };

    const html = renderReportHtml({
      cspSource: 'vscode-webview:',
      snapshot,
    });

    assert.ok(!html.includes('<script>evil()</script>'));
    assert.ok(!html.includes('<svg onload'));
    assert.ok(!html.includes('<iframe'));
    assert.ok(!html.includes('<b onmouseover'));
    assert.ok(!html.includes('<a href="javascript:'));
    assert.ok(!html.includes('test/<script>.cpp'));

    assert.ok(html.includes('&lt;script&gt;evil()&lt;/script&gt;'));
    assert.ok(html.includes('&lt;svg onload=alert(2)&gt;'));
    assert.ok(html.includes('test/&lt;script&gt;.cpp'));
  });
});

describe('reportHtml - Explanations and Semantics (BL-503)', () => {
  it('explains bound status and confidence separately with distinct descriptions', () => {
    const proven = explainBoundStatus('proven');
    const estimated = explainBoundStatus('estimated');
    const unknown = explainBoundStatus('unknown');
    const unsupported = explainBoundStatus('unsupported');

    assert.equal(proven.label, 'Proven');
    assert.ok(proven.explanation.includes('Mathematically verified'));
    assert.equal(estimated.label, 'Estimated');
    assert.ok(estimated.explanation.includes('heuristically'));
    assert.equal(unknown.label, 'Unknown');
    assert.ok(unknown.explanation.includes('Could not derive'));
    assert.equal(unsupported.label, 'Unsupported');
    assert.ok(unsupported.explanation.includes('outside current analyzer'));

    const high = explainConfidence('high');
    const none = explainConfidence('none');
    assert.equal(high.label, 'High');
    assert.ok(high.explanation.includes('Exact pattern match'));
    assert.equal(none.label, 'None');
    assert.ok(none.explanation.includes('No confidence'));
  });

  it('renders prominent alert box for unknown and unsupported results explaining their reasons', () => {
    const snapshot: ReportSnapshot = {
      displayPath: 'main.cpp',
      results: sampleResults(),
      engineVersion: '0.1.0',
    };

    const html = renderReportHtml({
      cspSource: 'vscode-webview:',
      snapshot,
    });

    // F2 is unknown: should have an alert box with class alert-unknown
    assert.ok(html.includes('class="alert-box alert-unknown"'));
    assert.ok(html.includes('Unknown Bound'));
    assert.ok(html.includes('Missing constraints for variable(s): <code>x</code>'));
    assert.ok(html.includes('data-dependent step condition'));
    assert.ok(html.includes('open mathematical problem'));
  });

  it('renders separate bound status and confidence dimension boxes in each card', () => {
    const snapshot: ReportSnapshot = {
      displayPath: 'main.cpp',
      results: sampleResults(),
      engineVersion: '0.1.0',
    };

    const html = renderReportHtml({
      cspSource: 'vscode-webview:',
      snapshot,
    });

    assert.ok(html.includes('class="status-confidence-grid"'));
    assert.ok(html.includes('Bound Status:'));
    assert.ok(html.includes('badge-status badge-proven'));
    assert.ok(html.includes('Confidence:'));
    assert.ok(html.includes('badge-confidence badge-high'));
    assert.ok(html.includes('Mathematically verified upper bound'));
  });

  it('renders time and space quantities with expressions, asymptotic, workload and method', () => {
    const snapshot: ReportSnapshot = {
      displayPath: 'main.cpp',
      results: sampleResults(),
      engineVersion: '0.1.0',
    };

    const html = renderReportHtml({
      cspSource: 'vscode-webview:',
      snapshot,
    });

    // Time
    assert.ok(html.includes('Time Complexity'));
    assert.ok(html.includes('<code>n</code>'));
    assert.ok(html.includes('O(n)'));
    assert.ok(html.includes('pointwise-maxima'));
    assert.ok(html.includes('≤ 10000'));

    // Space
    assert.ok(html.includes('Space Complexity'));
    assert.ok(html.includes('<code>4 * n</code>'));
    assert.ok(html.includes('linear-scaling'));
    assert.ok(html.includes('39.06 KiB')); // 40,000 bytes formatted
  });

  it('renders memory analysis lower bounds and items table', () => {
    const snapshot: ReportSnapshot = {
      displayPath: 'main.cpp',
      results: sampleResults(),
      engineVersion: '0.1.0',
    };

    const html = renderReportHtml({
      cspSource: 'vscode-webview:',
      snapshot,
    });

    assert.ok(html.includes('Memory Analysis (Lower Bounds)'));
    assert.ok(html.includes('Static Storage'));
    assert.ok(html.includes('Largest Local Array'));
    assert.ok(html.includes('1.00 KiB')); // 1024 bytes
    assert.ok(html.includes('Estimated Minimum'));
    assert.ok(html.includes('4.00 KiB')); // 4096 bytes
    assert.ok(html.includes('buffer'));
    assert.ok(html.includes('global static'));
    assert.ok(html.includes('int[1000]'));
    assert.ok(html.includes('flat memory model'));
  });

  it('renders feasibility assessment cards with status and reasons', () => {
    const snapshot: ReportSnapshot = {
      displayPath: 'main.cpp',
      results: sampleResults(),
      engineVersion: '0.1.0',
    };

    const html = renderReportHtml({
      cspSource: 'vscode-webview:',
      snapshot,
    });

    assert.ok(html.includes('Feasibility Assessment'));
    assert.ok(html.includes('Time Feasibility'));
    assert.ok(html.includes('workload is well within the 1-second budget'));
    assert.ok(html.includes('Memory Feasibility'));
    assert.ok(html.includes('4 KiB is well within the 256 MiB limit'));
  });
});

describe('reportHtml - Workload Chart (BL-503)', () => {
  it('renders pure inline SVG chart comparing finding workloads against scale max', () => {
    const findings: Finding[] = sampleResults().findings;
    const chart = renderWorkloadChart(findings, 10000);

    assert.ok(chart.includes('<svg'));
    assert.ok(chart.includes('class="workload-svg"'));
    assert.ok(chart.includes('role="img"'));
    assert.ok(chart.includes('aria-label="Workload comparison chart"'));

    // Check F1 dominant bar
    assert.ok(chart.includes('data-finding-id="F1"'));
    assert.ok(chart.includes('class="chart-bar bar-dominant"'));
    assert.ok(chart.includes('solve()'));
    assert.ok(chart.includes('≤ 10000'));

    // Check F2 unknown indicator
    assert.ok(chart.includes('data-finding-id="F2"'));
    assert.ok(chart.includes('class="chart-bar bar-unknown"'));
    assert.ok(chart.includes('unknown (missing: x)'));

    // Check scale max header
    assert.ok(chart.includes('Scale Max (10000)'));
  });

  it('returns empty string if findings list is empty or has no time findings', () => {
    assert.equal(renderWorkloadChart([], 1000), '');

    const noTimeFinding: Finding = {
      ...sampleResults().findings[0]!,
      time: null,
    };
    assert.equal(renderWorkloadChart([noTimeFinding], 1000), '');
  });
});
