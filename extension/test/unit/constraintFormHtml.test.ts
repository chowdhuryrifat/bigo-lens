import assert from 'node:assert/strict';
import { describe, it } from 'node:test';

import {
  buildCspMetaTag,
  escapeHtml,
  generateNonce,
  renderConstraintFormHtml,
} from '../../src/services/constraintFormHtml';
import type { ConstraintFormState } from '../../src/services/constraintsFormModel';

describe('constraintFormHtml - CSP and Nonce', () => {
  it('generates a cryptographically random base64 nonce of expected length', () => {
    const nonce1 = generateNonce();
    const nonce2 = generateNonce();
    assert.notEqual(nonce1, nonce2);
    assert.equal(typeof nonce1, 'string');
    assert.ok(nonce1.length >= 20);
  });

  it('builds CSP meta tag with all required strict directives', () => {
    const nonce = 'testnonce123';
    const tag = buildCspMetaTag('vscode-webview:', nonce);
    assert.ok(tag.includes("default-src 'none'"));
    assert.ok(tag.includes(`script-src 'nonce-${nonce}'`));
    assert.ok(tag.includes(`style-src vscode-webview: 'nonce-${nonce}'`));
    assert.ok(tag.includes('img-src vscode-webview: data:'));
    assert.ok(!tag.includes("'unsafe-inline'"));
    assert.ok(!tag.includes("'unsafe-eval'"));
  });
});

describe('constraintFormHtml - HTML Escaping and XSS Prevention', () => {
  it('escapes HTML special characters', () => {
    assert.equal(
      escapeHtml('<script>alert("xss")</script> & \'test\''),
      '&lt;script&gt;alert(&quot;xss&quot;)&lt;/script&gt; &amp; &#39;test&#39;',
    );
  });

  it('escapes dynamic strings in file paths and variable names', () => {
    const maliciousDisplayPath = 'test/path/<img onerror=alert(1)>.cpp';
    const maliciousSidecarPath = 'test/path/<svg onload=alert(2)>.json';
    const state: ConstraintFormState = {
      variables: [
        {
          name: 'var_<script>',
          min: '1',
          max: '100',
          detectedInSource: '<marquee>detected</marquee>',
        },
      ],
      sumConstraints: [],
      productConstraints: [],
      linearConstraints: [],
    };

    const html = renderConstraintFormHtml({
      cspSource: 'vscode-webview:',
      displayPath: maliciousDisplayPath,
      sidecarPath: maliciousSidecarPath,
      initialState: state,
    });

    assert.ok(!html.includes('<img onerror=alert(1)>'));
    assert.ok(!html.includes('<svg onload=alert(2)>'));
    assert.ok(!html.includes('<marquee>detected</marquee>'));
    assert.ok(html.includes('&lt;img onerror=alert(1)&gt;.cpp'));
    assert.ok(html.includes('&lt;svg onload=alert(2)&gt;.json'));
  });
});

describe('constraintFormHtml - Rendering Form Sections and Controls', () => {
  it('renders all required form sections, limits, and action buttons', () => {
    const state: ConstraintFormState = {
      variables: [
        { name: 'n', min: '1', max: '200000', detectedInSource: 'loop line 5' },
        { name: 't', min: '1', max: '10' },
      ],
      sumConstraints: [{ variable: 'n', count_variable: 't', max_total: '200000' }],
      productConstraints: [],
      linearConstraints: [],
      timeLimitMs: '2000',
      memoryLimitMb: '256',
    };

    const html = renderConstraintFormHtml({
      cspSource: 'vscode-webview:',
      scriptUri: 'vscode-webview://media/constraintForm.js',
      styleUri: 'vscode-webview://media/constraintForm.css',
      displayPath: 'src/solution.cpp',
      sidecarPath: 'src/solution.cpp.bigolens.json',
      initialState: state,
    });

    assert.ok(html.includes('BigO Lens — Constraints: src/solution.cpp'));
    assert.ok(html.includes('Sidecar target: <code>src/solution.cpp.bigolens.json</code>'));
    assert.ok(html.includes('id="btn-add-variable"'));
    assert.ok(html.includes('id="btn-add-sum"'));
    assert.ok(html.includes('id="btn-add-product"'));
    assert.ok(html.includes('id="btn-add-linear"'));
    assert.ok(html.includes('id="input-time-limit"'));
    assert.ok(html.includes('id="input-memory-limit"'));
    assert.ok(html.includes('id="btn-validate"'));
    assert.ok(html.includes('id="btn-save"'));
    assert.ok(html.includes('id="btn-save-analyze"'));
    assert.ok(html.includes('id="initial-state"'));
    assert.ok(html.includes('href="vscode-webview://media/constraintForm.css"'));
    assert.ok(html.includes('src="vscode-webview://media/constraintForm.js"'));
  });

  it('includes proper ARIA labels, section landmarks, and table scopes (BL-508)', () => {
    const state: ConstraintFormState = {
      variables: [],
      sumConstraints: [],
      productConstraints: [],
      linearConstraints: [],
    };

    const html = renderConstraintFormHtml({
      cspSource: 'vscode-webview:',
      displayPath: 'solution.cpp',
      sidecarPath: 'solution.cpp.bigolens.json',
      initialState: state,
    });

    assert.ok(html.includes('role="main"'));
    assert.ok(html.includes('aria-labelledby="heading-variables"'));
    assert.ok(html.includes('id="heading-variables"'));
    assert.ok(html.includes('aria-labelledby="heading-relationships"'));
    assert.ok(html.includes('id="heading-relationships"'));
    assert.ok(html.includes('aria-labelledby="heading-sum"'));
    assert.ok(html.includes('id="heading-sum"'));
    assert.ok(html.includes('aria-labelledby="heading-product"'));
    assert.ok(html.includes('id="heading-product"'));
    assert.ok(html.includes('aria-labelledby="heading-linear"'));
    assert.ok(html.includes('id="heading-linear"'));
    assert.ok(html.includes('aria-labelledby="heading-limits"'));
    assert.ok(html.includes('id="heading-limits"'));
    assert.ok(html.includes('role="region" aria-label="Detected unconstrained variables"'));
    assert.ok(html.includes('role="alert" aria-live="polite"'));
    assert.ok(html.includes('<th style="width: 25%;" scope="col">Variable Name</th>'));
    assert.ok(html.includes('<th style="width: 30%;" scope="col">Variable (n)</th>'));
    assert.ok(html.includes('<th style="width: 50%;" scope="col">Variables (e.g. n, m)</th>'));
    assert.ok(html.includes('<th style="width: 50%;" scope="col">Coefficients (e.g. n: 1, m: 2)</th>'));
  });
});
