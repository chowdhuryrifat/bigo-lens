import { randomBytes } from 'node:crypto';

import type { ConstraintFormState } from './constraintsFormModel';

export interface RenderConstraintFormHtmlOptions {
  cspSource: string;
  scriptUri?: string | undefined;
  styleUri?: string | undefined;
  nonce?: string | undefined;
  displayPath: string;
  sidecarPath: string;
  initialState: ConstraintFormState;
}

export function generateNonce(): string {
  return randomBytes(16).toString('base64');
}

export function escapeHtml(str: string): string {
  return str
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

export function buildCspMetaTag(cspSource: string, nonce: string): string {
  const policy = [
    "default-src 'none'",
    `script-src 'nonce-${nonce}'`,
    `style-src ${cspSource} 'nonce-${nonce}'`,
    `img-src ${cspSource} data:`,
    `font-src ${cspSource}`,
  ].join('; ');
  return `<meta http-equiv="Content-Security-Policy" content="${policy};">`;
}

/**
 * Renders the HTML for the BigO Lens Constraint Form webview.
 * Conforms to strict CSP rules: no inline scripts, all attributes/text strictly escaped.
 */
export function renderConstraintFormHtml(options: RenderConstraintFormHtmlOptions): string {
  const nonce = options.nonce ?? generateNonce();
  const cspMeta = buildCspMetaTag(options.cspSource, nonce);
  const styleLink = options.styleUri ? `<link rel="stylesheet" href="${escapeHtml(options.styleUri)}">` : '';
  const scriptTag = options.scriptUri
    ? `<script nonce="${nonce}" src="${escapeHtml(options.scriptUri)}"></script>`
    : '';

  const escapedDisplayPath = escapeHtml(options.displayPath);
  const escapedSidecarPath = escapeHtml(options.sidecarPath);
  const stateJson = escapeHtml(JSON.stringify(options.initialState));

  return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  ${cspMeta}
  <title>BigO Lens — Constraints: ${escapedDisplayPath}</title>
  ${styleLink}
</head>
<body class="constraint-form-body">
  <header class="form-header">
    <div class="header-main">
      <h1>BigO Lens — Constraints</h1>
      <span class="file-path">${escapedDisplayPath}</span>
    </div>
    <div class="sidecar-info">
      Sidecar target: <code>${escapedSidecarPath}</code>
    </div>
  </header>

  <div id="validation-banner" class="validation-banner hidden" role="alert"></div>

  <main class="form-content">
    <!-- Variables Section -->
    <section class="form-section" id="section-variables">
      <div class="section-header">
        <h2>Variables</h2>
        <button type="button" class="btn btn-secondary" id="btn-add-variable">+ Add Variable</button>
      </div>
      <p class="section-desc">
        Define lower and upper bounds for symbolic size variables. Inputs accept integers, scientific notation (e.g. <code>2e5</code>), or power expressions (e.g. <code>2*10^5</code>).
      </p>

      <div id="detected-quickadd" class="detected-quickadd hidden">
        <span class="quickadd-label">Detected in source:</span>
        <div id="quickadd-chips" class="quickadd-chips"></div>
      </div>

      <div class="table-container">
        <table class="form-table" id="table-variables">
          <thead>
            <tr>
              <th style="width: 25%;">Variable Name</th>
              <th style="width: 20%;">Min Bound</th>
              <th style="width: 20%;">Max Bound</th>
              <th style="width: 25%;">Detected in Source</th>
              <th style="width: 10%;">Actions</th>
            </tr>
          </thead>
          <tbody id="tbody-variables">
            <!-- Populated dynamically by script -->
          </tbody>
        </table>
      </div>
    </section>

    <!-- Relationships Section -->
    <section class="form-section" id="section-relationships">
      <div class="section-header">
        <h2>Relationships</h2>
        <div class="btn-group">
          <button type="button" class="btn btn-secondary" id="btn-add-sum">+ Add Sum Constraint</button>
          <button type="button" class="btn btn-secondary" id="btn-add-product">+ Add Product Constraint</button>
          <button type="button" class="btn btn-secondary" id="btn-add-linear">+ Add Linear Constraint</button>
        </div>
      </div>
      <p class="section-desc">
        Configure algebraic relationships across variables. Sum constraints are <strong>used</strong> to tighten aggregate loops; product and linear constraints are validated.
      </p>

      <!-- Sum Constraints -->
      <div class="subsection">
        <div class="subsection-header">
          <h3>Sum Constraints &nbsp;<span class="badge badge-used">used</span></h3>
        </div>
        <div class="table-container">
          <table class="form-table" id="table-sum">
            <thead>
              <tr>
                <th style="width: 30%;">Variable (n)</th>
                <th style="width: 30%;">Count Variable (t test cases)</th>
                <th style="width: 30%;">Max Total (&Sigma; n &le; S)</th>
                <th style="width: 10%;">Actions</th>
              </tr>
            </thead>
            <tbody id="tbody-sum">
              <!-- Populated dynamically -->
            </tbody>
          </table>
        </div>
      </div>

      <!-- Product Constraints -->
      <div class="subsection">
        <div class="subsection-header">
          <h3>Product Constraints &nbsp;<span class="badge badge-validated">validated</span></h3>
        </div>
        <div class="table-container">
          <table class="form-table" id="table-product">
            <thead>
              <tr>
                <th style="width: 50%;">Variables (e.g. n, m)</th>
                <th style="width: 40%;">Max Product (&Pi; v_i &le; P)</th>
                <th style="width: 10%;">Actions</th>
              </tr>
            </thead>
            <tbody id="tbody-product">
              <!-- Populated dynamically -->
            </tbody>
          </table>
        </div>
      </div>

      <!-- Linear Constraints -->
      <div class="subsection">
        <div class="subsection-header">
          <h3>Linear Constraints &nbsp;<span class="badge badge-validated">validated</span></h3>
        </div>
        <div class="table-container">
          <table class="form-table" id="table-linear">
            <thead>
              <tr>
                <th style="width: 50%;">Coefficients (e.g. n: 1, m: 2)</th>
                <th style="width: 40%;">Bound (&Sigma; c_i &middot; v_i &le; B)</th>
                <th style="width: 10%;">Actions</th>
              </tr>
            </thead>
            <tbody id="tbody-linear">
              <!-- Populated dynamically -->
            </tbody>
          </table>
        </div>
      </div>
    </section>

    <!-- Execution Limits Section -->
    <section class="form-section" id="section-limits">
      <div class="section-header">
        <h2>Limits</h2>
      </div>
      <p class="section-desc">
        Execution limits used for feasibility assessment and differential benchmark evaluation.
      </p>
      <div class="limits-grid">
        <div class="form-group">
          <label for="input-time-limit">Time Limit (ms)</label>
          <input type="text" id="input-time-limit" class="form-input" placeholder="2000" />
          <span class="input-hint">Range: 1 ms to 3,600,000 ms (1 hour)</span>
        </div>
        <div class="form-group">
          <label for="input-memory-limit">Memory Limit (MB)</label>
          <input type="text" id="input-memory-limit" class="form-input" placeholder="256" />
          <span class="input-hint">Range: 1 MB to 1,048,576 MB (1 TB)</span>
        </div>
      </div>
    </section>

    <!-- Actions Bar -->
    <div class="form-actions-bar">
      <button type="button" class="btn btn-secondary" id="btn-validate">Validate</button>
      <button type="button" class="btn btn-primary" id="btn-save">Save to Sidecar</button>
      <button type="button" class="btn btn-primary btn-accent" id="btn-save-analyze">Save &amp; Analyze</button>
    </div>
  </main>

  <script type="application/json" id="initial-state">${stateJson}</script>
  ${scriptTag}
</body>
</html>`;
}
