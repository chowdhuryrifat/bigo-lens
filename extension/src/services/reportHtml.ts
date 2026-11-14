import { randomBytes } from 'node:crypto';

import type {
  AnalyzeResults,
  BenchmarkData,
  BoundStatus,
  Confidence,
  Finding,
  Quantity,
} from '../models/protocol';
import { formatBytes, formatCount, statusLabel } from './format';

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

/**
 * Explains what each bound status mathematically guarantees.
 */
export function explainBoundStatus(status: BoundStatus): { label: string; explanation: string } {
  switch (status) {
    case 'proven':
      return {
        label: 'Proven',
        explanation:
          'Mathematically verified upper bound derived from loop bounds, step patterns, or recurrence relations.',
      };
    case 'estimated':
      return {
        label: 'Estimated',
        explanation:
          'Empirically or heuristically derived bound (e.g. from standard library cost models or input distribution assumptions).',
      };
    case 'unknown':
      return {
        label: 'Unknown',
        explanation:
          'Could not derive an upper bound. The loop condition may be data-dependent, or required variable constraints are missing.',
      };
    case 'unsupported':
      return {
        label: 'Unsupported',
        explanation:
          'The code construct (e.g. multiple direct recursion, complex pointer arithmetic) is outside current analyzer capabilities.',
      };
  }
}

/**
 * Explains what each confidence rating indicates about analyzer certainty.
 */
export function explainConfidence(confidence: Confidence): { label: string; explanation: string } {
  switch (confidence) {
    case 'high':
      return {
        label: 'High',
        explanation: 'Exact pattern match with formal AST semantics and no unverified assumptions.',
      };
    case 'medium':
      return {
        label: 'Medium',
        explanation: 'Syntactic structure matched with simplifying assumptions or inferred constraints.',
      };
    case 'low':
      return {
        label: 'Low',
        explanation:
          'Heuristic approximation; significant risk that actual behavior diverges from the estimate.',
      };
    case 'none':
      return {
        label: 'None',
        explanation: 'No confidence in bound derivation due to missing bounds or unsupported constructs.',
      };
  }
}

type FindingWithTime = Finding & { time: Quantity };

/**
 * Renders an inline, pure SVG workload-versus-budget horizontal bar chart.
 * Requires zero external libraries, adhering strictly to CSP directives.
 */
export function renderWorkloadChart(findings: Finding[], maxWorkload: number | null): string {
  if (findings.length === 0) {
    return '';
  }

  const chartable = findings.filter((f): f is FindingWithTime => f.time !== null);
  if (chartable.length === 0) {
    return '';
  }

  let scaleMax = maxWorkload && maxWorkload > 0 ? maxWorkload : 1;
  for (const f of chartable) {
    const val = f.time.workload.upper_bound;
    if (val !== null && val > scaleMax) {
      scaleMax = val;
    }
  }

  const rowHeight = 44;
  const topPadding = 40;
  const bottomPadding = 30;
  const totalHeight = topPadding + chartable.length * rowHeight + bottomPadding;
  const chartWidth = 680;
  const labelWidth = 160;
  const barAreaWidth = 370;

  const rows = chartable.map((f, idx) => {
    const y = topPadding + idx * rowHeight;
    const ub = f.time.workload.upper_bound;
    const isUnknown = ub === null;
    const isDominant = maxWorkload !== null && ub === maxWorkload;
    const funcName = f.function ? `${f.function}()` : f.pattern;

    let barSvg: string;
    let valText: string;

    if (isUnknown) {
      const missing = f.time.workload.missing_bounds.join(', ');
      valText = missing ? `unknown (missing: ${missing})` : 'unknown bound';
      barSvg = `
        <rect x="${labelWidth}" y="${y + 6}" width="16" height="20" rx="3" class="chart-bar bar-unknown" />
      `;
    } else {
      const ratio = scaleMax > 0 ? Math.min(1.0, ub / scaleMax) : 0;
      const barWidth = Math.max(6, ratio * barAreaWidth);
      valText = `≤ ${formatCount(ub)}`;
      const barClass = isDominant ? 'chart-bar bar-dominant' : 'chart-bar bar-normal';
      barSvg = `
        <rect x="${labelWidth}" y="${y + 6}" width="${barWidth}" height="20" rx="3" class="${barClass}" />
      `;
    }

    return `
      <g class="chart-row" data-finding-id="${escapeHtml(f.id)}">
        <text x="${labelWidth - 10}" y="${y + 20}" text-anchor="end" class="chart-label-id">${escapeHtml(f.id)}</text>
        <text x="${labelWidth - 40}" y="${y + 20}" text-anchor="end" class="chart-label-func">${escapeHtml(funcName)}</text>
        ${barSvg}
        <text x="${labelWidth + barAreaWidth + 15}" y="${y + 20}" text-anchor="start" class="chart-val-text ${isUnknown ? 'val-unknown' : ''}">${escapeHtml(valText)}</text>
      </g>
    `;
  });

  return `
    <div class="workload-chart-card">
      <div class="chart-header">
        <h3>Workload Comparison</h3>
        <span class="chart-subtitle">Upper bounds evaluated under active constraints</span>
      </div>
      <div class="chart-svg-container">
        <svg viewBox="0 0 ${chartWidth} ${totalHeight}" width="100%" height="${totalHeight}" class="workload-svg" role="img" aria-label="Workload comparison chart">
          <line x1="${labelWidth}" y1="${topPadding - 10}" x2="${labelWidth}" y2="${totalHeight - bottomPadding + 5}" class="chart-axis" />
          <line x1="${labelWidth + barAreaWidth}" y1="${topPadding - 10}" x2="${labelWidth + barAreaWidth}" y2="${totalHeight - bottomPadding + 5}" class="chart-axis-grid" />
          <text x="${labelWidth}" y="${topPadding - 18}" text-anchor="start" class="axis-header">0</text>
          <text x="${labelWidth + barAreaWidth}" y="${topPadding - 18}" text-anchor="end" class="axis-header">Scale Max (${formatCount(scaleMax)})</text>
          ${rows.join('\n')}
        </svg>
      </div>
    </div>
  `;
}

function renderProminentAlert(finding: Finding): string {
  if (finding.bound_status !== 'unknown' && finding.bound_status !== 'unsupported') {
    return '';
  }

  const isUnknown = finding.bound_status === 'unknown';
  const alertClass = isUnknown ? 'alert-unknown' : 'alert-unsupported';
  const title = isUnknown ? 'Unknown Bound' : 'Unsupported Construct';
  const reasons: string[] = [];

  if (finding.time && finding.time.workload.missing_bounds.length > 0) {
    reasons.push(
      `Missing constraints for variable(s): <code>${finding.time.workload.missing_bounds.map(escapeHtml).join(', ')}</code>`,
    );
  }
  if (finding.limitations.length > 0) {
    reasons.push(...finding.limitations.map(escapeHtml));
  }
  if (reasons.length === 0) {
    reasons.push(escapeHtml(finding.description));
  }

  const leadReason = reasons[0] ?? escapeHtml(finding.description);
  const extraReasons = reasons.slice(1);

  return `
    <div class="alert-box ${alertClass}" role="alert">
      <div class="alert-lead">
        <span class="alert-badge">${escapeHtml(title)}</span>
        <span class="alert-message">${leadReason}</span>
      </div>
      ${
        extraReasons.length > 0
          ? `<ul class="alert-details">${extraReasons.map((r) => `<li>${r}</li>`).join('')}</ul>`
          : ''
      }
    </div>
  `;
}

function renderFindingCard(finding: Finding, displayPath: string): string {
  const funcDisplay = finding.function
    ? `<span class="finding-func">${escapeHtml(finding.function)}()</span>`
    : '';
  const fileTarget = finding.location.file || displayPath;
  const line = finding.location.line;
  const col = finding.location.column;

  const boundInfo = explainBoundStatus(finding.bound_status);
  const confInfo = explainConfidence(finding.confidence);

  const prominentAlertHtml = renderProminentAlert(finding);

  // Time details
  let timeSection = '';
  if (finding.time) {
    const t = finding.time;
    const asymptotic = t.asymptotic ? escapeHtml(t.asymptotic) : 'not derived';
    const expression = escapeHtml(t.expression);
    const varsList =
      t.variables.length > 0 ? t.variables.map((v) => `<code>${escapeHtml(v)}</code>`).join(', ') : 'none';

    const w = t.workload;
    const ubText = w.upper_bound !== null ? `&le; ${formatCount(w.upper_bound)}` : 'unknown';
    const methodText = w.method ? escapeHtml(w.method) : 'none';
    const missingText = w.missing_bounds.length > 0 ? escapeHtml(w.missing_bounds.join(', ')) : 'none';

    timeSection = `
      <div class="finding-quantity-block">
        <h4 class="quantity-title">Time Complexity</h4>
        <table class="finding-table">
          <tbody>
            <tr>
              <th scope="row">Expression</th>
              <td><code>${expression}</code></td>
              <th scope="row">Asymptotic</th>
              <td><strong class="asymptotic-tag">${asymptotic}</strong></td>
            </tr>
            <tr>
              <th scope="row">Variables</th>
              <td>${varsList}</td>
              <th scope="row">Workload</th>
              <td><strong>${ubText}</strong> <span class="workload-status status-${escapeHtml(w.status)}">[${escapeHtml(statusLabel(w.status))}]</span></td>
            </tr>
            <tr>
              <th scope="row">Method</th>
              <td>${methodText}</td>
              <th scope="row">Missing Bounds</th>
              <td>${missingText}</td>
            </tr>
          </tbody>
        </table>
      </div>
    `;
  }

  // Space details
  let spaceSection = '';
  if (finding.space) {
    const s = finding.space;
    const sAsymptotic = s.asymptotic ? escapeHtml(s.asymptotic) : 'not derived';
    const sExpression = escapeHtml(s.expression);
    const sw = s.workload;
    const sUbText = sw.upper_bound !== null ? `&le; ${formatBytes(sw.upper_bound)}` : 'unknown';

    spaceSection = `
      <div class="finding-quantity-block">
        <h4 class="quantity-title">Space Complexity</h4>
        <table class="finding-table">
          <tbody>
            <tr>
              <th scope="row">Expression</th>
              <td><code>${sExpression}</code></td>
              <th scope="row">Asymptotic</th>
              <td><strong class="asymptotic-tag">${sAsymptotic}</strong></td>
            </tr>
            <tr>
              <th scope="row">Workload</th>
              <td><strong>${sUbText}</strong> <span class="workload-status status-${escapeHtml(sw.status)}">[${escapeHtml(statusLabel(sw.status))}]</span></td>
              <th scope="row">Method</th>
              <td>${sw.method ? escapeHtml(sw.method) : 'none'}</td>
            </tr>
          </tbody>
        </table>
      </div>
    `;
  }

  // Bound status vs confidence (explained separately)
  const separateExplanationsHtml = `
    <div class="status-confidence-grid">
      <div class="dimension-box">
        <div class="dimension-header">
          <span class="dimension-name">Bound Status:</span>
          <span class="badge badge-status badge-${escapeHtml(finding.bound_status)}">${escapeHtml(boundInfo.label)}</span>
        </div>
        <p class="dimension-desc">${escapeHtml(boundInfo.explanation)}</p>
      </div>
      <div class="dimension-box">
        <div class="dimension-header">
          <span class="dimension-name">Confidence:</span>
          <span class="badge badge-confidence badge-${escapeHtml(finding.confidence)}">${escapeHtml(confInfo.label)}</span>
        </div>
        <p class="dimension-desc">${escapeHtml(confInfo.explanation)}</p>
      </div>
    </div>
  `;

  const assumptionsHtml =
    finding.assumptions.length > 0
      ? `<div class="finding-notes"><strong>Assumptions:</strong><ul>${finding.assumptions.map((a) => `<li>${escapeHtml(a)}</li>`).join('')}</ul></div>`
      : '';

  const limitationsHtml =
    finding.limitations.length > 0
      ? `<div class="finding-notes limitations"><strong>Limitations:</strong><ul>${finding.limitations.map((l) => `<li>${escapeHtml(l)}</li>`).join('')}</ul></div>`
      : '';

  return `
    <section class="finding-card card-${escapeHtml(finding.bound_status)}" id="finding-${escapeHtml(finding.id)}">
      <div class="finding-header">
        <div class="finding-title">
          <span class="finding-id">${escapeHtml(finding.id)}</span>
          <span class="category-badge category-${escapeHtml(finding.category)}">${escapeHtml(finding.category.replace('_', ' '))}</span>
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
      ${prominentAlertHtml}
      <p class="finding-desc">${escapeHtml(finding.description)}</p>
      ${separateExplanationsHtml}
      ${timeSection}
      ${spaceSection}
      ${assumptionsHtml}
      ${limitationsHtml}
      <div class="finding-footer">
        <span class="provenance-tag">Provenance: ${escapeHtml(finding.provenance)}</span>
      </div>
    </section>
  `;
}

function renderMemorySection(memory: AnalyzeResults['memory']): string {
  const staticBytes = formatBytes(memory.static_storage_bytes);
  const localArrayBytes = formatBytes(memory.largest_local_array_bytes);
  const estimatedMin = formatBytes(memory.estimated_minimum_bytes);
  const statusBadge = `<span class="badge badge-status badge-${escapeHtml(memory.estimated_minimum_status)}">${escapeHtml(statusLabel(memory.estimated_minimum_status))}</span>`;

  const itemsHtml =
    memory.items.length > 0
      ? `
        <table class="report-table memory-items-table">
          <thead>
            <tr>
              <th scope="col">Name</th>
              <th scope="col">Kind</th>
              <th scope="col">Type</th>
              <th scope="col">Bytes</th>
              <th scope="col">Location</th>
            </tr>
          </thead>
          <tbody>
            ${memory.items
              .map((item) => {
                const bytesStr =
                  item.exact_bytes !== null
                    ? formatBytes(item.exact_bytes)
                    : item.symbolic_bytes
                      ? escapeHtml(item.symbolic_bytes)
                      : '—';
                return `
                  <tr>
                    <td><code>${escapeHtml(item.name)}</code></td>
                    <td><span class="kind-tag">${escapeHtml(item.kind.replace('_', ' '))}</span></td>
                    <td><code>${escapeHtml(item.type)}</code></td>
                    <td>${bytesStr}</td>
                    <td>line ${item.location.line}</td>
                  </tr>
                `;
              })
              .join('')}
          </tbody>
        </table>
      `
      : '<p class="text-muted">No individual static or buffer items tracked.</p>';

  const assumptionsHtml =
    memory.assumptions.length > 0
      ? `<div class="section-notes"><strong>Memory Assumptions:</strong><ul>${memory.assumptions.map((a) => `<li>${escapeHtml(a)}</li>`).join('')}</ul></div>`
      : '';

  return `
    <section class="report-section memory-section" id="section-memory">
      <h2>Memory Analysis (Lower Bounds)</h2>
      <div class="metrics-grid">
        <div class="metric-card">
          <span class="metric-label">Static Storage</span>
          <span class="metric-value">${staticBytes}</span>
          ${memory.static_storage_saturated ? '<span class="metric-sub text-warning">Saturated</span>' : ''}
        </div>
        <div class="metric-card">
          <span class="metric-label">Largest Local Array</span>
          <span class="metric-value">${localArrayBytes}</span>
        </div>
        <div class="metric-card">
          <span class="metric-label">Estimated Minimum</span>
          <span class="metric-value">${estimatedMin}</span>
          <span class="metric-sub">${statusBadge}</span>
        </div>
      </div>
      <h3>Storage Items</h3>
      ${itemsHtml}
      ${assumptionsHtml}
    </section>
  `;
}

function renderFeasibilitySection(feasibility: AnalyzeResults['feasibility']): string {
  const time = feasibility.time;
  const memory = feasibility.memory;

  return `
    <section class="report-section feasibility-section" id="section-feasibility">
      <h2>Feasibility Assessment</h2>
      <div class="feasibility-grid">
        <div class="feasibility-card feasibility-${escapeHtml(time.status)}">
          <div class="feasibility-card-header">
            <h3>Time Feasibility</h3>
            <span class="badge badge-feasibility badge-feasibility-${escapeHtml(time.status)}">
              ${escapeHtml(time.status.toUpperCase().replace('_', ' '))}
            </span>
          </div>
          <p class="feasibility-reason">${escapeHtml(time.reason)}</p>
        </div>
        <div class="feasibility-card feasibility-${escapeHtml(memory.status)}">
          <div class="feasibility-card-header">
            <h3>Memory Feasibility</h3>
            <span class="badge badge-feasibility badge-feasibility-${escapeHtml(memory.status)}">
              ${escapeHtml(memory.status.toUpperCase().replace('_', ' '))}
            </span>
          </div>
          <p class="feasibility-reason">${escapeHtml(memory.reason)}</p>
        </div>
      </div>
    </section>
  `;
}

export function formatMilliseconds(ms: number): string {
  if (ms < 0.001) {
    return `${(ms * 1000).toFixed(1)} \u00b5s`;
  }
  if (ms < 1) {
    return `${ms.toFixed(3)} ms`;
  }
  if (ms < 1000) {
    return `${ms.toFixed(2)} ms`;
  }
  return `${(ms / 1000).toFixed(2)} s`;
}

export function formatSizeRecord(record: Record<string, number>): string {
  const entries = Object.entries(record);
  if (entries.length === 0) {
    return '-';
  }
  return entries.map(([k, v]) => `${k} = ${formatCount(v)}`).join(', ');
}

/**
 * Pure SVG runtime-range chart plotting measured sample points with MAD error bars,
 * empirical fit curve, and target extrapolated range.
 */
export function renderRuntimeRangeChart(benchmark: BenchmarkData): string {
  if (benchmark.samples.length === 0) {
    return '';
  }

  const sortedSamples = [...benchmark.samples].sort((a, b) => {
    const aVal = Object.values(a.size)[0] ?? 0;
    const bVal = Object.values(b.size)[0] ?? 0;
    return aVal - bVal;
  });

  const ext = benchmark.extrapolated_runtime;
  const targetSizeVal =
    Object.values(ext.target_size)[0] ??
    Object.values(sortedSamples[sortedSamples.length - 1]?.size ?? {})[0] ??
    0;

  const sampleSizes = sortedSamples.map((s) => Object.values(s.size)[0] ?? 0);
  const maxSampleX = Math.max(...sampleSizes, 1);
  const maxX = Math.max(targetSizeVal, maxSampleX) * 1.05;

  const sampleMaxTimes = sortedSamples.map((s) => s.wall_time_median_ms + s.wall_time_mad_ms);
  const maxMeasuredY = Math.max(...sampleMaxTimes, 0.001);
  const maxY = Math.max(ext.max_ms, maxMeasuredY) * 1.1;

  const leftPad = 70;
  const rightPad = 30;
  const topPad = 30;
  const bottomPad = 45;
  const chartW = 600;
  const chartH = 260;
  const plotW = chartW - leftPad - rightPad;
  const plotH = chartH - topPad - bottomPad;

  const scaleX = (x: number): number => leftPad + (Math.max(0, x) / maxX) * plotW;
  const scaleY = (y: number): number => topPad + plotH - (Math.max(0, y) / maxY) * plotH;

  const yAxisZero = scaleY(0);
  const yAxisMid = scaleY(maxY / 2);
  const yAxisTop = scaleY(maxY);

  const xAxisZero = scaleX(0);
  const xAxisMid = scaleX(maxX / 2);
  const xAxisRight = scaleX(maxX);

  const points = sortedSamples.map((s) => {
    const sizeVal = Object.values(s.size)[0] ?? 0;
    return {
      x: scaleX(sizeVal),
      y: scaleY(s.wall_time_median_ms),
      sizeVal,
      median: s.wall_time_median_ms,
      mad: s.wall_time_mad_ms,
    };
  });

  const pathD = points.map((p, i) => `${i === 0 ? 'M' : 'L'} ${p.x.toFixed(1)} ${p.y.toFixed(1)}`).join(' ');

  const targetX = scaleX(targetSizeVal);
  const targetY = scaleY(ext.central_ms);
  const targetMinY = scaleY(ext.min_ms);
  const targetMaxY = scaleY(ext.max_ms);

  const lastPoint = points[points.length - 1];
  const extrapPathD = lastPoint
    ? `M ${lastPoint.x.toFixed(1)} ${lastPoint.y.toFixed(1)} L ${targetX.toFixed(1)} ${targetY.toFixed(1)}`
    : '';

  const errorBars = points
    .map((p) => {
      const topY = scaleY(p.median + p.mad);
      const botY = scaleY(Math.max(0, p.median - p.mad));
      return `
        <line x1="${p.x.toFixed(1)}" y1="${topY.toFixed(1)}" x2="${p.x.toFixed(1)}" y2="${botY.toFixed(1)}" class="chart-error-bar" />
        <line x1="${(p.x - 3).toFixed(1)}" y1="${topY.toFixed(1)}" x2="${(p.x + 3).toFixed(1)}" y2="${topY.toFixed(1)}" class="chart-error-cap" />
        <line x1="${(p.x - 3).toFixed(1)}" y1="${botY.toFixed(1)}" x2="${(p.x + 3).toFixed(1)}" y2="${botY.toFixed(1)}" class="chart-error-cap" />
      `;
    })
    .join('');

  const circles = points
    .map(
      (p) => `
        <circle cx="${p.x.toFixed(1)}" cy="${p.y.toFixed(1)}" r="4" class="chart-sample-point">
          <title>Size: ${formatCount(p.sizeVal)}, Median: ${formatMilliseconds(p.median)} \u00b1 ${formatMilliseconds(p.mad)} (MAD)</title>
        </circle>
      `,
    )
    .join('');

  return `
    <div class="chart-container benchmark-chart-container">
      <h3 class="chart-title">Empirical Runtime &amp; Extrapolation Curve</h3>
      <svg
        viewBox="0 0 ${chartW} ${chartH}"
        class="benchmark-chart-svg"
        role="img"
        aria-label="Empirical runtime and extrapolated range chart"
      >
        <!-- Grid lines -->
        <line x1="${leftPad}" y1="${yAxisZero.toFixed(1)}" x2="${(leftPad + plotW).toFixed(1)}" y2="${yAxisZero.toFixed(1)}" class="chart-axis-line" />
        <line x1="${leftPad}" y1="${yAxisMid.toFixed(1)}" x2="${(leftPad + plotW).toFixed(1)}" y2="${yAxisMid.toFixed(1)}" class="chart-grid-line" />
        <line x1="${leftPad}" y1="${yAxisTop.toFixed(1)}" x2="${(leftPad + plotW).toFixed(1)}" y2="${yAxisTop.toFixed(1)}" class="chart-grid-line" />
        <line x1="${leftPad}" y1="${topPad}" x2="${leftPad}" y2="${(topPad + plotH).toFixed(1)}" class="chart-axis-line" />

        <!-- Y Axis Labels -->
        <text x="${(leftPad - 8).toFixed(1)}" y="${yAxisZero.toFixed(1)}" class="chart-tick-label" text-anchor="end" dominant-baseline="middle">0 ms</text>
        <text x="${(leftPad - 8).toFixed(1)}" y="${yAxisMid.toFixed(1)}" class="chart-tick-label" text-anchor="end" dominant-baseline="middle">${formatMilliseconds(maxY / 2)}</text>
        <text x="${(leftPad - 8).toFixed(1)}" y="${yAxisTop.toFixed(1)}" class="chart-tick-label" text-anchor="end" dominant-baseline="middle">${formatMilliseconds(maxY)}</text>
        <text x="${leftPad}" y="${(topPad - 12).toFixed(1)}" class="chart-axis-header">Wall Time</text>

        <!-- X Axis Labels -->
        <text x="${xAxisZero.toFixed(1)}" y="${(yAxisZero + 18).toFixed(1)}" class="chart-tick-label" text-anchor="middle">0</text>
        <text x="${xAxisMid.toFixed(1)}" y="${(yAxisZero + 18).toFixed(1)}" class="chart-tick-label" text-anchor="middle">${formatCount(Math.round(maxX / 2))}</text>
        <text x="${xAxisRight.toFixed(1)}" y="${(yAxisZero + 18).toFixed(1)}" class="chart-tick-label" text-anchor="middle">${formatCount(Math.round(maxX))}</text>
        <text x="${(leftPad + plotW / 2).toFixed(1)}" y="${(yAxisZero + 36).toFixed(1)}" class="chart-axis-header" text-anchor="middle">Input Size (n)</text>

        <!-- Curve: measured samples -->
        <path d="${pathD}" class="chart-fit-line" />

        <!-- Error bars and sample dots -->
        ${errorBars}
        ${circles}

        <!-- Curve: extrapolated to target -->
        ${extrapPathD ? `<path d="${extrapPathD}" class="chart-extrapolated-line" stroke-dasharray="4,4" />` : ''}

        <!-- Target range bar and caps -->
        <line x1="${targetX.toFixed(1)}" y1="${targetMinY.toFixed(1)}" x2="${targetX.toFixed(1)}" y2="${targetMaxY.toFixed(1)}" class="chart-target-range-bar" stroke-dasharray="2,2" />
        <line x1="${(targetX - 4).toFixed(1)}" y1="${targetMinY.toFixed(1)}" x2="${(targetX + 4).toFixed(1)}" y2="${targetMinY.toFixed(1)}" class="chart-target-cap" />
        <line x1="${(targetX - 4).toFixed(1)}" y1="${targetMaxY.toFixed(1)}" x2="${(targetX + 4).toFixed(1)}" y2="${targetMaxY.toFixed(1)}" class="chart-target-cap" />

        <!-- Target central point -->
        <polygon
          points="${targetX.toFixed(1)},${(targetY - 5).toFixed(1)} ${(targetX + 5).toFixed(1)},${targetY.toFixed(1)} ${targetX.toFixed(1)},${(targetY + 5).toFixed(1)} ${(targetX - 5).toFixed(1)},${targetY.toFixed(1)}"
          class="chart-target-diamond"
        >
          <title>Extrapolated Target Size: ${formatCount(targetSizeVal)}, Central: ${formatMilliseconds(ext.central_ms)} [${formatMilliseconds(ext.min_ms)} \u2013 ${formatMilliseconds(ext.max_ms)}]</title>
        </polygon>

        <!-- Legend -->
        <g class="chart-legend" transform="translate(${(leftPad + 10).toFixed(1)}, ${(topPad + 10).toFixed(1)})">
          <circle cx="5" cy="5" r="4" class="chart-sample-point" />
          <text x="14" y="8" class="chart-legend-text">Measured (Median \u00b1 MAD)</text>
          <polygon points="175,0 180,5 175,10 170,5" class="chart-target-diamond" />
          <text x="188" y="8" class="chart-legend-text">Extrapolated Range</text>
        </g>
      </svg>
    </div>
  `;
}

export function renderBenchmarkSection(benchmark: BenchmarkData): string {
  const chartHtml = renderRuntimeRangeChart(benchmark);
  const isIsolated = benchmark.isolated;
  const isoBadgeClass = isIsolated ? 'badge-isolated' : 'badge-unisolated';
  const isoBadgeText = isIsolated
    ? `ISOLATED (${escapeHtml(benchmark.isolation_level)})`
    : `NOT SANDBOXED (${escapeHtml(benchmark.isolation_level)})`;

  const fit = benchmark.empirical_fit;
  const ext = benchmark.extrapolated_runtime;

  let warningsHtml = '';
  if (benchmark.warnings.length > 0) {
    warningsHtml = `
      <div class="benchmark-warnings">
        ${benchmark.warnings.map((w) => `<div class="alert alert-warning">${escapeHtml(w)}</div>`).join('')}
      </div>
    `;
  }

  let ambiguousHtml = '';
  if (fit.ambiguous) {
    ambiguousHtml = `
      <div class="alert alert-warning">
        <strong>Ambiguous Growth Fit:</strong> Multiple complexity models fit the empirical timings similarly. Extrapolated runtimes carry higher uncertainty.
      </div>
    `;
  }

  const maxRssBytes = benchmark.samples.reduce((max, s) => Math.max(max, s.peak_rss_bytes), 0);

  return `
    <section class="report-section benchmark-section" id="section-benchmark">
      <div class="section-header-row">
        <h2>Empirical Benchmark &amp; Runtime Range</h2>
        <span class="badge ${isoBadgeClass}">${isoBadgeText}</span>
      </div>
      ${warningsHtml}
      ${ambiguousHtml}
      <div class="benchmark-cards-grid">
        <div class="benchmark-card">
          <span class="benchmark-card-title">Extrapolated Runtime</span>
          <div class="benchmark-card-main">${formatMilliseconds(ext.central_ms)}</div>
          <div class="benchmark-card-sub">Range: ${formatMilliseconds(ext.min_ms)} &ndash; ${formatMilliseconds(ext.max_ms)}</div>
          <div class="benchmark-card-detail">Target: ${formatSizeRecord(ext.target_size)} &bull; Extrapolated: ${ext.extrapolated ? 'Yes' : 'No'}</div>
        </div>
        <div class="benchmark-card">
          <span class="benchmark-card-title">Empirical Growth Fit</span>
          <div class="benchmark-card-main">${escapeHtml(fit.model ?? 'unknown')}</div>
          <div class="benchmark-card-sub">R&sup2; = ${fit.r_squared.toFixed(4)} &bull; RSS = ${fit.rss.toExponential(3)}</div>
          <div class="benchmark-card-detail">Slope: ${fit.slope.toFixed(4)} &bull; Intercept: ${fit.intercept.toFixed(4)}</div>
        </div>
        <div class="benchmark-card">
          <span class="benchmark-card-title">Peak Memory (RSS)</span>
          <div class="benchmark-card-main">${formatBytes(maxRssBytes)}</div>
          <div class="benchmark-card-sub">Measured across ${benchmark.samples.length} sample sizes</div>
          <div class="benchmark-card-detail">Max RSS across benchmark child runs</div>
        </div>
      </div>
      ${chartHtml}
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
      maxWorkloadText = `&le; ${escapeHtml(formatCount(summary.max_time_workload))}`;
    }

    const timeFeas = escapeHtml(snap.results.feasibility.time.status.toUpperCase().replace('_', ' '));
    const memFeas = escapeHtml(snap.results.feasibility.memory.status.toUpperCase().replace('_', ' '));

    const chartHtml = renderWorkloadChart(findings, summary.max_time_workload);
    const benchmarkHtml = snap.results.benchmark ? renderBenchmarkSection(snap.results.benchmark) : '';
    const memoryHtml = renderMemorySection(snap.results.memory);
    const feasibilityHtml = renderFeasibilitySection(snap.results.feasibility);

    const findingsHtml =
      findings.length > 0
        ? findings.map((f) => renderFindingCard(f, snap.displayPath)).join('\n')
        : '<p class="no-findings">No loop nests, recursion or storage findings detected.</p>';

    bodyContent = `
      <header class="report-header">
        <div class="header-main">
          <h1>BigO Lens Report &mdash; <span class="file-path">${escapeHtml(snap.displayPath)}</span></h1>
          <div class="header-actions">
            <span class="engine-badge">Engine v${escapeHtml(snap.engineVersion)}</span>
            <button class="action-btn benchmark-btn" data-action="run-benchmark" title="Run benchmark on local machine (requires explicit confirmation)">Run Benchmark</button>
          </div>
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
            <span class="summary-value badge-feasibility badge-feasibility-${escapeHtml(snap.results.feasibility.time.status)}">${timeFeas}</span>
          </div>
          <div class="summary-item">
            <span class="summary-label">Memory Feasibility</span>
            <span class="summary-value badge-feasibility badge-feasibility-${escapeHtml(snap.results.feasibility.memory.status)}">${memFeas}</span>
          </div>
        </div>
      </header>
      <main class="report-main">
        ${chartHtml}
        ${benchmarkHtml}
        ${feasibilityHtml}
        ${memoryHtml}
        <section class="report-section findings-section" id="section-findings">
          <h2>Findings Details (${totalFindings})</h2>
          <div class="findings-list">
            ${findingsHtml}
          </div>
        </section>
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
