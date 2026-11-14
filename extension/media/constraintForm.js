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
    validationBanner.replaceChildren();

    const strong = document.createElement('strong');
    strong.textContent = message;
    validationBanner.appendChild(strong);

    if (list && list.length > 0) {
      const ul = document.createElement('ul');
      for (const item of list) {
        const li = document.createElement('li');
        li.textContent = item;
        ul.appendChild(li);
      }
      validationBanner.appendChild(ul);
    }
    validationBanner.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
  }

  function hideBanner() {
    if (!validationBanner) return;
    validationBanner.className = 'validation-banner hidden';
    validationBanner.replaceChildren();
  }

  // --- Rendering Rows with Safe DOM Construction (No innerHTML) ---

  function renderVariableRow(v) {
    if (!tbodyVariables) return;
    const tr = document.createElement('tr');
    tr.className = 'row-variable';

    const tdName = document.createElement('td');
    const inputName = document.createElement('input');
    inputName.type = 'text';
    inputName.className = 'table-input input-var-name';
    inputName.placeholder = 'e.g. n, v.size()';
    inputName.value = v && v.name ? v.name : '';
    tdName.appendChild(inputName);

    const tdMin = document.createElement('td');
    const inputMin = document.createElement('input');
    inputMin.type = 'text';
    inputMin.className = 'table-input input-var-min';
    inputMin.placeholder = 'e.g. 1';
    inputMin.value = v && v.min !== undefined ? v.min : '';
    tdMin.appendChild(inputMin);

    const tdMax = document.createElement('td');
    const inputMax = document.createElement('input');
    inputMax.type = 'text';
    inputMax.className = 'table-input input-var-max';
    inputMax.placeholder = 'e.g. 2e5, 200000';
    inputMax.value = v && v.max !== undefined ? v.max : '';
    tdMax.appendChild(inputMax);

    const tdDetected = document.createElement('td');
    const detectedSpan = document.createElement('span');
    detectedSpan.className = 'detected-label';
    if (v && v.detectedInSource) {
      const badge = document.createElement('span');
      badge.className = 'badge badge-detected';
      badge.textContent = v.detectedInSource;
      detectedSpan.appendChild(badge);
    } else {
      const empty = document.createElement('span');
      empty.style.opacity = '0.5';
      empty.textContent = '—';
      detectedSpan.appendChild(empty);
    }
    tdDetected.appendChild(detectedSpan);

    const tdActions = document.createElement('td');
    const btnDelete = document.createElement('button');
    btnDelete.type = 'button';
    btnDelete.className = 'btn btn-danger btn-delete-row';
    btnDelete.title = 'Delete variable';
    btnDelete.textContent = '×';
    btnDelete.addEventListener('click', () => {
      tr.remove();
      updateQuickaddChips();
    });
    tdActions.appendChild(btnDelete);

    tr.appendChild(tdName);
    tr.appendChild(tdMin);
    tr.appendChild(tdMax);
    tr.appendChild(tdDetected);
    tr.appendChild(tdActions);

    tbodyVariables.appendChild(tr);
  }

  function renderSumRow(sc) {
    if (!tbodySum) return;
    const tr = document.createElement('tr');
    tr.className = 'row-sum';

    const tdVar = document.createElement('td');
    const inputVar = document.createElement('input');
    inputVar.type = 'text';
    inputVar.className = 'table-input input-sum-var';
    inputVar.placeholder = 'e.g. n';
    inputVar.value = sc && sc.variable ? sc.variable : '';
    tdVar.appendChild(inputVar);

    const tdCount = document.createElement('td');
    const inputCount = document.createElement('input');
    inputCount.type = 'text';
    inputCount.className = 'table-input input-sum-count';
    inputCount.placeholder = 'e.g. t';
    inputCount.value = sc && sc.count_variable ? sc.count_variable : '';
    tdCount.appendChild(inputCount);

    const tdTotal = document.createElement('td');
    const inputTotal = document.createElement('input');
    inputTotal.type = 'text';
    inputTotal.className = 'table-input input-sum-total';
    inputTotal.placeholder = 'e.g. 2e5, 200000';
    inputTotal.value = sc && sc.max_total ? sc.max_total : '';
    tdTotal.appendChild(inputTotal);

    const tdActions = document.createElement('td');
    const btnDelete = document.createElement('button');
    btnDelete.type = 'button';
    btnDelete.className = 'btn btn-danger btn-delete-row';
    btnDelete.title = 'Delete sum constraint';
    btnDelete.textContent = '×';
    btnDelete.addEventListener('click', () => {
      tr.remove();
    });
    tdActions.appendChild(btnDelete);

    tr.appendChild(tdVar);
    tr.appendChild(tdCount);
    tr.appendChild(tdTotal);
    tr.appendChild(tdActions);

    tbodySum.appendChild(tr);
  }

  function renderProductRow(pc) {
    if (!tbodyProduct) return;
    const tr = document.createElement('tr');
    tr.className = 'row-product';

    let varsStr = '';
    if (pc && pc.variables) {
      varsStr = Array.isArray(pc.variables) ? pc.variables.join(', ') : pc.variables;
    }

    const tdVars = document.createElement('td');
    const inputVars = document.createElement('input');
    inputVars.type = 'text';
    inputVars.className = 'table-input input-prod-vars';
    inputVars.placeholder = 'e.g. n, m';
    inputVars.value = varsStr;
    tdVars.appendChild(inputVars);

    const tdMax = document.createElement('td');
    const inputMax = document.createElement('input');
    inputMax.type = 'text';
    inputMax.className = 'table-input input-prod-max';
    inputMax.placeholder = 'e.g. 1e6, 1000000';
    inputMax.value = pc && pc.max_product ? pc.max_product : '';
    tdMax.appendChild(inputMax);

    const tdActions = document.createElement('td');
    const btnDelete = document.createElement('button');
    btnDelete.type = 'button';
    btnDelete.className = 'btn btn-danger btn-delete-row';
    btnDelete.title = 'Delete product constraint';
    btnDelete.textContent = '×';
    btnDelete.addEventListener('click', () => {
      tr.remove();
    });
    tdActions.appendChild(btnDelete);

    tr.appendChild(tdVars);
    tr.appendChild(tdMax);
    tr.appendChild(tdActions);

    tbodyProduct.appendChild(tr);
  }

  function renderLinearRow(lc) {
    if (!tbodyLinear) return;
    const tr = document.createElement('tr');
    tr.className = 'row-linear';

    let coeffStr = '';
    if (lc && lc.coefficients) {
      coeffStr = Object.entries(lc.coefficients)
        .map(([k, val]) => `${k}: ${val}`)
        .join(', ');
    }

    const tdCoeffs = document.createElement('td');
    const inputCoeffs = document.createElement('input');
    inputCoeffs.type = 'text';
    inputCoeffs.className = 'table-input input-lin-coeffs';
    inputCoeffs.placeholder = 'e.g. n: 1, m: 2';
    inputCoeffs.value = coeffStr;
    tdCoeffs.appendChild(inputCoeffs);

    const tdBound = document.createElement('td');
    const inputBound = document.createElement('input');
    inputBound.type = 'text';
    inputBound.className = 'table-input input-lin-bound';
    inputBound.placeholder = 'e.g. 300000';
    inputBound.value = lc && lc.bound ? lc.bound : '';
    tdBound.appendChild(inputBound);

    const tdActions = document.createElement('td');
    const btnDelete = document.createElement('button');
    btnDelete.type = 'button';
    btnDelete.className = 'btn btn-danger btn-delete-row';
    btnDelete.title = 'Delete linear constraint';
    btnDelete.textContent = '×';
    btnDelete.addEventListener('click', () => {
      tr.remove();
    });
    tdActions.appendChild(btnDelete);

    tr.appendChild(tdCoeffs);
    tr.appendChild(tdBound);
    tr.appendChild(tdActions);

    tbodyLinear.appendChild(tr);
  }

  // --- Quickadd chips for detected variables ---

  function updateQuickaddChips() {
    if (!quickaddBox || !quickaddChips || !tbodyVariables) return;
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
    quickaddChips.replaceChildren();

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
    if (tbodyVariables) {
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
    }

    const sumConstraints = [];
    if (tbodySum) {
      tbodySum.querySelectorAll('.row-sum').forEach((r) => {
        const variable = (r.querySelector('.input-sum-var')?.value || '').trim();
        const count_variable = (r.querySelector('.input-sum-count')?.value || '').trim();
        const max_total = (r.querySelector('.input-sum-total')?.value || '').trim();
        if (variable || count_variable || max_total) {
          sumConstraints.push({ variable, count_variable, max_total });
        }
      });
    }

    const productConstraints = [];
    if (tbodyProduct) {
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
    }

    const linearConstraints = [];
    if (tbodyLinear) {
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
    }

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

  // --- Handle Extension Messages with Origin Check ---

  window.addEventListener('message', (event) => {
    if (event.origin !== window.origin && event.origin !== location.origin) {
      return;
    }
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
