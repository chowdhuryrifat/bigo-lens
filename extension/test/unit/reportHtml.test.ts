import assert from 'node:assert/strict';
import { describe, it } from 'node:test';

import type { AnalyzeResults } from '../../src/models/protocol';
import {
  buildCspMetaTag,
  escapeHtml,
  generateNonce,
  renderReportHtml,
  type ReportSnapshot,
} from '../../src/services/reportHtml';

function sampleResults(): AnalyzeResults {
  return {
    summary: {
      finding_count: 1,
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
            method: 'maxima-evaluation',
            assumptions: ['n <= 10000'],
            missing_bounds: [],
          },
        },
        space: null,
        bound_status: 'proven',
        confidence: 'high',
        assumptions: ['n <= 10000'],
        limitations: [],
        provenance: 'clang-loop-analyzer',
      },
    ],
    memory: {
      static_storage_bytes: 4,
      static_storage_saturated: false,
      largest_local_array_bytes: 0,
      estimated_minimum_bytes: 4,
      estimated_minimum_status: 'proven',
      assumptions: [],
      items: [],
    },
    feasibility: {
      time: { status: 'likely_safe', reason: 'within time budget' },
      memory: { status: 'likely_safe', reason: 'within memory budget' },
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

    // Must be escaped into &lt;img onerror=alert(1)&gt;
    assert.ok(
      html.includes('&lt;img onerror=alert(1)&gt;'),
      'The escaped payload must be present as text in the HTML',
    );
    // Raw unescaped tag must NOT exist
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

describe('reportHtml - Structural Rendering', () => {
  it('renders an empty state when no snapshot is provided', () => {
    const html = renderReportHtml({ cspSource: 'vscode-webview:' });

    assert.ok(html.includes('<!DOCTYPE html>'));
    assert.ok(html.includes('No Analysis Available'));
    assert.ok(html.includes('BigO Lens: Analyze Active File'));
  });

  it('renders complete report when snapshot is provided', () => {
    const snapshot: ReportSnapshot = {
      displayPath: 'main.cpp',
      results: sampleResults(),
      engineVersion: '1.2.3',
    };

    const html = renderReportHtml({
      cspSource: 'vscode-webview:',
      snapshot,
      styleUri: 'vscode-webview://media/report.css',
      scriptUri: 'vscode-webview://media/report.js',
    });

    assert.ok(html.includes('BigO Lens Report &mdash; <span class="file-path">main.cpp</span>'));
    assert.ok(html.includes('Engine v1.2.3'));
    assert.ok(html.includes('id="finding-F1"'));
    assert.ok(html.includes('data-action="navigate"'));
    assert.ok(html.includes('data-line="10"'));
    assert.ok(html.includes('O(n)'));
    assert.ok(html.includes('badge-proven'));
    assert.ok(html.includes('<link rel="stylesheet" href="vscode-webview://media/report.css">'));
    assert.ok(html.includes('<script nonce="'));
    assert.ok(html.includes('src="vscode-webview://media/report.js"'));
  });
});
