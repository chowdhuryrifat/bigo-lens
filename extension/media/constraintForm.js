/* BigO Lens Constraint Form Client Script */
(function () {
  const vscode = acquireVsCodeApi();

  // Read initial state safely embedded in script tag
  let state = {
    variables: [],
    sumConstraints: [],
    productConstraints: [],
    linearConstraints: [],
    timeLimitMs: '',
    memoryLimitMb: '',
    detectedVariables: [],
  };

  const stateElement = document.getElementById('initial-state');
  if (stateElement && stateElement.textContent) {
    try {
      state = JSON.parse(stateElement.textContent);
    } catch {
      // Fallback to default empty state
    }
  }

  const tbodyVariables = document.getElementById('tbody-variables');
  const tbodySum = document.getElementById('tbody-sum');
  const tbodyProduct = document.getElementById('tbody-product');
  const tbodyLinear = document.getElementById('tbody-linear');
  const inputTime = document.getElementById('input-time-limit');
  const inputMemory = document.getElementById('input-memory-limit');
  const validationBanner = document.getElementById('validation-banner');
  const quickaddBox = document.getElementById('detected-quickadd');
  const quickaddChips = document.getElementById('quickadd-chips');

  function showBanner(type, message, list) {
    if (!validationBanner) return;
    validationBanner.className = `validation-banner banner-${type}`;
    let html = `<strong>${message}</strong>`;
    if (list && list.length > 0) {
      html += '<ul>';
      for (const item of list) {
        html += `<li>${escapeHtml(item)}</li>`;
      }
      html += '</ul>';
    }
    validationBanner.innerHTML = html;
    validationBanner.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
  }

  function hideBanner() {
    if (!validationBanner) return;
    validationBanner.className = 'validation-banner hidden';
    validationBanner.innerHTML = '';
  }

  function escapeHtml(str) {
    if (typeof str !== 'string') return '';
    return str
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;')
      .replace(/'/g, '&#39;');
  }

  // --- Rendering Rows ---

  function renderVariableRow(v) {
    const tr = document.createElement('tr');
    tr.className = 'row-variable';

    const name = v && v.name ? v.name : '';
    const min = v && v.min !== undefined ? v.min : '';
    const max = v && v.max !== undefined ? v.max : '';
    const detected = v && v.detectedInSource ? v.detectedInSource : '';

    tr.innerHTML = `
      <td><input type="text" class="table-input input-var-name" placeholder="e.g. n, v.size()" value="${escapeHtml(name)}" /></td>
      <td><input type="text" class="table-input input-var-min" placeholder="e.g. 1" value="${escapeHtml(min)}" /></td>
      <td><input type="text" class="table-input input-var-max" placeholder="e.g. 2e5, 200000" value="${escapeHtml(max)}" /></td>
      <td><span class="detected-label">${detected ? `<span class="badge badge-detected">${escapeHtml(detected)}</span>` : '<span style="opacity: 0.5;">—</span>'}</span></td>
      <td><button type="button" class="btn btn-danger btn-delete-row" title="Delete variable">&times;</button></td>
    `;

    tr.querySelector('.btn-delete-row').addEventListener('click', () => {
      tr.remove();
      updateQuickaddChips();
    });

    tbodyVariables.appendChild(tr);
  }

  function renderSumRow(sc) {
    const tr = document.createElement('tr');
    tr.className = 'row-sum';

    const v = sc && sc.variable ? sc.variable : '';
    const cv = sc && sc.count_variable ? sc.count_variable : '';
    const total = sc && sc.max_total ? sc.max_total : '';

    tr.innerHTML = `
      <td><input type="text" class="table-input input-sum-var" placeholder="e.g. n" value="${escapeHtml(v)}" /></td>
      <td><input type="text" class="table-input input-sum-count" placeholder="e.g. t" value="${escapeHtml(cv)}" /></td>
      <td><input type="text" class="table-input input-sum-total" placeholder="e.g. 2e5, 200000" value="${escapeHtml(total)}" /></td>
      <td><button type="button" class="btn btn-danger btn-delete-row" title="Delete sum constraint">&times;</button></td>
    `;

    tr.querySelector('.btn-delete-row').addEventListener('click', () => {
      tr.remove();
    });

    tbodySum.appendChild(tr);
  }

  function renderProductRow(pc) {
    const tr = document.createElement('tr');
    tr.className = 'row-product';

    let varsStr = '';
    if (pc && pc.variables) {
      varsStr = Array.isArray(pc.variables) ? pc.variables.join(', ') : pc.variables;
    }
    const maxProd = pc && pc.max_product ? pc.max_product : '';

    tr.innerHTML = `
      <td><input type="text" class="table-input input-prod-vars" placeholder="e.g. n, m" value="${escapeHtml(varsStr)}" /></td>
      <td><input type="text" class="table-input input-prod-max" placeholder="e.g. 1e6, 1000000" value="${escapeHtml(maxProd)}" /></td>
      <td><button type="button" class="btn btn-danger btn-delete-row" title="Delete product constraint">&times;</button></td>
    `;

    tr.querySelector('.btn-delete-row').addEventListener('click', () => {
      tr.remove();
    });

    tbodyProduct.appendChild(tr);
  }

  function renderLinearRow(lc) {
    const tr = document.createElement('tr');
    tr.className = 'row-linear';

    let coeffStr = '';
    if (lc && lc.coefficients) {
      coeffStr = Object.entries(lc.coefficients)
        .map(([k, val]) => `${k}: ${val}`)
        .join(', ');
    }
    const bound = lc && lc.bound ? lc.bound : '';

    tr.innerHTML = `
      <td><input type="text" class="table-input input-lin-coeffs" placeholder="e.g. n: 1, m: 2" value="${escapeHtml(coeffStr)}" /></td>
      <td><input type="text" class="table-input input-lin-bound" placeholder="e.g. 300000" value="${escapeHtml(bound)}" /></td>
      <td><button type="button" class="btn btn-danger btn-delete-row" title="Delete linear constraint">&times;</button></td>
    `;

    tr.querySelector('.btn-delete-row').addEventListener('click', () => {
      tr.remove();
    });

    tbodyLinear.appendChild(tr);
  }

  // --- Quickadd chips for detected variables ---

  function updateQuickaddChips() {
    if (!quickaddBox || !quickaddChips) return;
    const detected = state.detectedVariables || [];
    if (detected.length === 0) {
      quickaddBox.classList.add('hidden');
      return;
    }

    // Collect currently configured variable names
    const existing = new Set();
    const rows = tbodyVariables.querySelectorAll('.row-variable');
    rows.forEach((r) => {
      const nameInput = r.querySelector('.input-var-name');
      if (nameInput && nameInput.value.trim()) {
        existing.add(nameInput.value.trim());
      }
    });

    const unconfigured = detected.filter((dv) => !existing.has(dv.name));
    if (unconfigured.length === 0) {
      quickaddBox.classList.add('hidden');
      return;
    }

    quickaddBox.classList.remove('hidden');
    quickaddChips.innerHTML = '';

    unconfigured.forEach((dv) => {
      const btn = document.createElement('button');
      btn.type = 'button';
      btn.className = `chip-btn ${dv.status === 'missing_bound' ? 'chip-missing' : ''}`;
      const statusIcon = dv.status === 'missing_bound' ? '⚠ ' : '+ ';
      btn.textContent = `${statusIcon}${dv.name}${dv.locationHint ? ` (${dv.locationHint})` : ''}`;
      btn.addEventListener('click', () => {
        renderVariableRow({
          name: dv.name,
          min: '1',
          max: '',
          detectedInSource: dv.locationHint || dv.name,
        });
        updateQuickaddChips();
      });
      quickaddChips.appendChild(btn);
    });
  }

  // --- Initial population ---

  if (state.variables && state.variables.length > 0) {
    state.variables.forEach(renderVariableRow);
  } else {
    // Add default row if empty
    renderVariableRow({ name: 'n', min: '1', max: '200000' });
  }

  if (state.sumConstraints && state.sumConstraints.length > 0) {
    state.sumConstraints.forEach(renderSumRow);
  }
  if (state.productConstraints && state.productConstraints.length > 0) {
    state.productConstraints.forEach(renderProductRow);
  }
  if (state.linearConstraints && state.linearConstraints.length > 0) {
    state.linearConstraints.forEach(renderLinearRow);
  }

  if (inputTime && state.timeLimitMs) {
    inputTime.value = state.timeLimitMs;
  }
  if (inputMemory && state.memoryLimitMb) {
    inputMemory.value = state.memoryLimitMb;
  }

  updateQuickaddChips();

  // --- Form Gathering ---

  function gatherFormState() {
    const variables = [];
    tbodyVariables.querySelectorAll('.row-variable').forEach((r) => {
      const name = (r.querySelector('.input-var-name')?.value || '').trim();
      const min = (r.querySelector('.input-var-min')?.value || '').trim();
      const max = (r.querySelector('.input-var-max')?.value || '').trim();
      const detected = r.querySelector('.detected-label')?.textContent?.trim() || undefined;
      if (name) {
        variables.push({
          name,
          min: min || undefined,
          max: max || undefined,
          detectedInSource: detected && detected !== '—' ? detected : undefined,
        });
      }
    });

    const sumConstraints = [];
    tbodySum.querySelectorAll('.row-sum').forEach((r) => {
      const variable = (r.querySelector('.input-sum-var')?.value || '').trim();
      const count_variable = (r.querySelector('.input-sum-count')?.value || '').trim();
      const max_total = (r.querySelector('.input-sum-total')?.value || '').trim();
      if (variable || count_variable || max_total) {
        sumConstraints.push({ variable, count_variable, max_total });
      }
    });

    const productConstraints = [];
    tbodyProduct.querySelectorAll('.row-product').forEach((r) => {
      const rawVars = (r.querySelector('.input-prod-vars')?.value || '').trim();
      const max_product = (r.querySelector('.input-prod-max')?.value || '').trim();
      if (rawVars || max_product) {
        const vars = rawVars
          .split(/[,\s]+/)
          .map((s) => s.trim())
          .filter((s) => s.length > 0);
        productConstraints.push({ variables: vars, max_product });
      }
    });

    const linearConstraints = [];
    tbodyLinear.querySelectorAll('.row-linear').forEach((r) => {
      const rawCoeffs = (r.querySelector('.input-lin-coeffs')?.value || '').trim();
      const bound = (r.querySelector('.input-lin-bound')?.value || '').trim();
      if (rawCoeffs || bound) {
        const coefficients = {};
        rawCoeffs.split(',').forEach((part) => {
          const [k, v] = part.split(':').map((s) => s.trim());
          if (k && v) {
            coefficients[k] = v;
          }
        });
        linearConstraints.push({ coefficients, bound });
      }
    });

    const timeLimitMs = (inputTime?.value || '').trim();
    const memoryLimitMb = (inputMemory?.value || '').trim();

    return {
      variables,
      sumConstraints,
      productConstraints,
      linearConstraints,
      timeLimitMs: timeLimitMs || undefined,
      memoryLimitMb: memoryLimitMb || undefined,
      detectedVariables: state.detectedVariables,
    };
  }

  // --- Button Event Handlers ---

  document.getElementById('btn-add-variable')?.addEventListener('click', () => {
    renderVariableRow();
  });
  document.getElementById('btn-add-sum')?.addEventListener('click', () => {
    renderSumRow();
  });
  document.getElementById('btn-add-product')?.addEventListener('click', () => {
    renderProductRow();
  });
  document.getElementById('btn-add-linear')?.addEventListener('click', () => {
    renderLinearRow();
  });

  document.getElementById('btn-validate')?.addEventListener('click', () => {
    hideBanner();
    const currentState = gatherFormState();
    vscode.postMessage({
      type: 'validate',
      state: currentState,
    });
  });

  document.getElementById('btn-save')?.addEventListener('click', () => {
    hideBanner();
    const currentState = gatherFormState();
    vscode.postMessage({
      type: 'save',
      state: currentState,
      andAnalyze: false,
    });
  });

  document.getElementById('btn-save-analyze')?.addEventListener('click', () => {
    hideBanner();
    const currentState = gatherFormState();
    vscode.postMessage({
      type: 'save',
      state: currentState,
      andAnalyze: true,
    });
  });

  // --- Handle Extension Messages ---

  window.addEventListener('message', (event) => {
    const message = event.data;
    if (!message || typeof message.type !== 'string') return;

    switch (message.type) {
      case 'validationResult': {
        if (message.ok) {
          showBanner('success', '✓ All constraints are valid and conform to the protocol schema.');
        } else {
          showBanner(
            'error',
            'Constraint validation failed with the following errors:',
            message.errors || ['Invalid constraints input'],
          );
        }
        break;
      }
      case 'saveResult': {
        if (message.ok) {
          showBanner(
            'success',
            `✓ Constraints successfully saved to sidecar${message.andAnalyze ? ' and analysis started.' : '.'}`,
          );
        } else {
          showBanner('error', `Failed to save constraints: ${message.error || 'Unknown error'}`);
        }
        break;
      }
    }
  });

  // Notify extension ready
  vscode.postMessage({ type: 'ready' });
})();
