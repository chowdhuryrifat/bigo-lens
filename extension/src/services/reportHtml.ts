import { randomBytes } from 'node:crypto';

import type { AnalyzeResults, Finding } from '../models/protocol';

export interface ReportSnapshot {
  displayPath: string;
  results: AnalyzeResults;
  engineVersion: string;
}

export interface RenderReportHtmlOptions {
  cspSource: string;
  scriptUri?: string | undefined;
  styleUri?: string | undefined;
  nonce?: string | undefined;
  snapshot?: ReportSnapshot | undefined;
}

/**
 * Generates a cryptographically strong 128-bit random base64 nonce.
 */
export function generateNonce(): string {
  return randomBytes(16).toString('base64');
}

/**
 * Strictly escapes HTML special characters to prevent XSS.
 * All dynamic strings inserted into the HTML document must be passed through this function.
 */
export function escapeHtml(str: string): string {
  return str
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

/**
 * Builds the strict Content Security Policy meta tag required for the report webview.
 * Disallows remote origins, inline scripts, and eval.
 */
export function buildCspMetaTag(cspSource: string, nonce: string): string {
  const policy = [
    "default-src 'none'",
    `script-src 'nonce-${nonce}'`,
    `style-src ${cspSource} 'nonce-${nonce}'`,
    `img-src ${cspSource}`,
    `font-src ${cspSource}`,
  ].join('; ');
  return `<meta http-equiv="Content-Security-Policy" content="${policy};">`;
}

function renderFindingCard(finding: Finding, displayPath: string): string {
  const funcDisplay = finding.function
    ? `<span class="finding-func">${escapeHtml(finding.function)}()</span>`
    : '';
  const fileTarget = finding.location.file || displayPath;
  const line = finding.location.line;
  const col = finding.location.column;

  const asymptotic = finding.time?.asymptotic ? escapeHtml(finding.time.asymptotic) : '—';
  const expression = finding.time?.expression ? escapeHtml(finding.time.expression) : '—';
  const boundStatus = escapeHtml(finding.bound_status.toUpperCase());
  const confidence = escapeHtml(finding.confidence.toUpperCase());

  const workloadObj = finding.time?.workload;
  const method = workloadObj?.method ? escapeHtml(workloadObj.method) : undefined;
  const boundVal =
    workloadObj?.upper_bound !== null && workloadObj?.upper_bound !== undefined
      ? escapeHtml(String(workloadObj.upper_bound))
      : undefined;

  let workloadText = '—';
  if (boundVal !== undefined) {
    workloadText = `&le; ${boundVal}`;
    if (method) {
      workloadText += ` <span class="workload-method">(method: ${method})</span>`;
    }
  }

  const assumptionsHtml =
    finding.assumptions.length > 0
      ? `<div class="finding-notes"><strong>Assumptions:</strong><ul>${finding.assumptions.map((a) => `<li>${escapeHtml(a)}</li>`).join('')}</ul></div>`
      : '';

  const limitationsHtml =
    finding.limitations.length > 0
      ? `<div class="finding-notes limitations"><strong>Limitations:</strong><ul>${finding.limitations.map((l) => `<li>${escapeHtml(l)}</li>`).join('')}</ul></div>`
      : '';

  return `
    <section class="finding-card" id="finding-${escapeHtml(finding.id)}">
      <div class="finding-header">
        <div class="finding-title">
          <span class="finding-id">${escapeHtml(finding.id)}</span>
          <span class="finding-pattern">${escapeHtml(finding.pattern)}</span>
          ${funcDisplay}
          <span class="finding-loc">line ${line}</span>
        </div>
        <button
          type="button"
          class="btn-navigate"
          data-action="navigate"
          data-file="${escapeHtml(fileTarget)}"
          data-line="${line}"
          data-column="${col}"
        >
          Go to source
        </button>
      </div>
      <p class="finding-desc">${escapeHtml(finding.description)}</p>
      <table class="finding-table">
        <tbody>
          <tr>
            <th scope="row">Expression</th>
            <td><code>${expression}</code></td>
            <th scope="row">Asymptotic</th>
            <td><strong>${asymptotic}</strong></td>
          </tr>
          <tr>
            <th scope="row">Workload</th>
            <td>${workloadText}</td>
            <th scope="row">Bound</th>
            <td><span class="badge badge-${escapeHtml(finding.bound_status)}">${boundStatus}</span> (${confidence} confidence)</td>
          </tr>
        </tbody>
      </table>
      ${assumptionsHtml}
      ${limitationsHtml}
    </section>
  `;
}

/**
 * Generates the complete HTML document for the BigO Lens report webview with strict CSP and escaping.
 */
export function renderReportHtml(options: RenderReportHtmlOptions): string {
  const nonce = options.nonce ?? generateNonce();
  const cspMeta = buildCspMetaTag(options.cspSource, nonce);

  const styleLink = options.styleUri ? `<link rel="stylesheet" href="${escapeHtml(options.styleUri)}">` : '';
  const scriptTag = options.scriptUri
    ? `<script nonce="${nonce}" src="${escapeHtml(options.scriptUri)}"></script>`
    : '';

  let bodyContent: string;
  if (!options.snapshot) {
    bodyContent = `
      <div class="empty-state">
        <h2>No Analysis Available</h2>
        <p>Run <strong>BigO Lens: Analyze Active File</strong> on a supported source file to view the complexity report.</p>
      </div>
    `;
  } else {
    const snap = options.snapshot;
    const findings = snap.results.findings;
    const summary = snap.results.summary;
    const totalFindings = summary.finding_count;
    const coverage = escapeHtml(summary.coverage);
    const dominantId = summary.dominant_finding_id ? escapeHtml(summary.dominant_finding_id) : 'none';

    let maxWorkloadText = 'unknown';
    if (summary.max_time_workload !== null) {
      maxWorkloadText = `&le; ${escapeHtml(String(summary.max_time_workload))}`;
    }

    const timeFeas = escapeHtml(snap.results.feasibility.time.status.toUpperCase());
    const memFeas = escapeHtml(snap.results.feasibility.memory.status.toUpperCase());

    const findingsHtml =
      findings.length > 0
        ? findings.map((f) => renderFindingCard(f, snap.displayPath)).join('\n')
        : '<p class="no-findings">No loop nests or functions detected for complexity analysis.</p>';

    bodyContent = `
      <header class="report-header">
        <div class="header-main">
          <h1>BigO Lens Report &mdash; <span class="file-path">${escapeHtml(snap.displayPath)}</span></h1>
          <span class="engine-badge">Engine v${escapeHtml(snap.engineVersion)}</span>
        </div>
        <div class="summary-bar">
          <div class="summary-item">
            <span class="summary-label">Findings</span>
            <span class="summary-value">${totalFindings}</span>
          </div>
          <div class="summary-item">
            <span class="summary-label">Coverage</span>
            <span class="summary-value">${coverage}</span>
          </div>
          <div class="summary-item">
            <span class="summary-label">Dominant</span>
            <span class="summary-value">${dominantId}</span>
          </div>
          <div class="summary-item">
            <span class="summary-label">Max Workload</span>
            <span class="summary-value">${maxWorkloadText}</span>
          </div>
          <div class="summary-item">
            <span class="summary-label">Time Feasibility</span>
            <span class="summary-value badge-feasibility">${timeFeas}</span>
          </div>
          <div class="summary-item">
            <span class="summary-label">Memory Feasibility</span>
            <span class="summary-value badge-feasibility">${memFeas}</span>
          </div>
        </div>
      </header>

      <main class="findings-container">
        <h2>Complexity Findings</h2>
        ${findingsHtml}
      </main>
    `;
  }

  return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  ${cspMeta}
  <title>BigO Lens Report</title>
  ${styleLink}
</head>
<body class="report-body">
  ${bodyContent}
  ${scriptTag}
</body>
</html>`;
}
