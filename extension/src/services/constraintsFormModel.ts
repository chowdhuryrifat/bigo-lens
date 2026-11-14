import type { Constraints, VariableBound } from '../models/protocol';

/** Valid C++ identifier optionally followed by .size() */
export const VARIABLE_NAME_PATTERN = '^[A-Za-z_][A-Za-z0-9_]{0,63}(\\.size\\(\\))?$';
export const VARIABLE_NAME_REGEX = /^[A-Za-z_][A-Za-z0-9_]{0,63}(\.size\(\))?$/;

/** Max magnitude allowed by protocol schema (10^18) */
export const MAX_NUMERIC_MAGNITUDE = 1e18;

/** Limits constraints from schema */
export const TIME_LIMIT_MS_MIN = 1;
export const TIME_LIMIT_MS_MAX = 3600000;
export const MEMORY_LIMIT_MB_MIN = 1;
export const MEMORY_LIMIT_MB_MAX = 1048576;

export type NumericParseResult = { ok: true; value: number } | { ok: false; error: string };

/**
 * Parses numeric input supporting standard integers, scientific notation,
 * and power multiplication expressions (e.g. "2*10^5", "10^5", "2.5e3").
 * Rejects non-integers, out-of-range values, NaNs, and infinities.
 */
export function parseNumericInput(raw: string): NumericParseResult {
  if (typeof raw !== 'string') {
    return { ok: false, error: 'Input must be a string' };
  }
  const trimmed = raw.trim();
  if (trimmed === '') {
    return { ok: false, error: 'Input cannot be empty' };
  }

  // Power multiplication expressions:
  // e.g. "2*10^5", "10^5", "3*10^4", "2 * 10^5", "10**5", "2 * 10 ^ 5"
  const powMulMatch = /^([+-]?(?:\d+(?:\.\d+)?|\.\d+))\s*(?:\*|×)\s*10\s*(?:\^|\*\*)\s*([+-]?\d+)$/.exec(
    trimmed,
  );
  const powMatch = /^([+-]?)\s*10\s*(?:\^|\*\*)\s*([+-]?\d+)$/.exec(trimmed);

  let num: number;
  if (powMulMatch) {
    const coeff = powMulMatch[1] ?? '1';
    const exp = powMulMatch[2] ?? '0';
    num = Number(`${coeff}e${exp}`);
  } else if (powMatch) {
    const sign = powMatch[1] ?? '';
    const exp = powMatch[2] ?? '0';
    num = Number(`${sign}1e${exp}`);
  } else {
    const stdMatch = /^[+-]?(?:\d+(?:\.\d+)?|\.\d+)(?:[eE][+-]?\d+)?$/.test(trimmed);
    if (!stdMatch) {
      return { ok: false, error: `Invalid numeric format: "${raw}"` };
    }
    num = Number(trimmed);
  }

  if (!Number.isFinite(num)) {
    return { ok: false, error: `Value must be a finite number: "${raw}"` };
  }
  if (!Number.isInteger(num)) {
    return { ok: false, error: `Value must be an integer: "${raw}"` };
  }
  if (Math.abs(num) > MAX_NUMERIC_MAGNITUDE) {
    return { ok: false, error: `Value exceeds maximum allowed magnitude (10^18): "${raw}"` };
  }
  return { ok: true, value: num === 0 ? 0 : num };
}

export interface FormVariableRow {
  name: string;
  min?: string;
  max?: string;
  detectedInSource?: string;
}

export interface FormSumConstraintRow {
  variable: string;
  count_variable: string;
  max_total: string;
}

export interface FormProductConstraintRow {
  variables: string[] | string;
  max_product: string;
}

export interface FormLinearConstraintRow {
  coefficients: Record<string, string>;
  bound: string;
}

export interface DetectedVariable {
  name: string;
  status: 'used' | 'missing_bound';
  locationHint?: string;
}

export interface ConstraintFormState {
  variables: FormVariableRow[];
  sumConstraints: FormSumConstraintRow[];
  productConstraints: FormProductConstraintRow[];
  linearConstraints: FormLinearConstraintRow[];
  timeLimitMs?: string;
  memoryLimitMb?: string;
  detectedVariables?: DetectedVariable[];
}

export type FormToConstraintsResult =
  { ok: true; constraints: Constraints } | { ok: false; errors: string[] };

/**
 * Creates an empty constraint form state with empty arrays.
 */
export function createEmptyFormState(): ConstraintFormState {
  return {
    variables: [],
    sumConstraints: [],
    productConstraints: [],
    linearConstraints: [],
  };
}

/**
 * Validates a variable name identifier against the protocol specification.
 */
export function validateVariableName(name: string): { ok: true } | { ok: false; error: string } {
  if (typeof name !== 'string' || !VARIABLE_NAME_REGEX.test(name)) {
    return {
      ok: false,
      error: `Invalid variable name "${name}": must match identifier pattern ^[A-Za-z_][A-Za-z0-9_]{0,63}(\\.size\\(\\))?$`,
    };
  }
  return { ok: true };
}

/**
 * Converts a sidecar Constraints object to the ConstraintFormState used by the UI.
 * Lossless round-trip with formToConstraints guaranteed for any valid Constraints object.
 */
export function constraintsToForm(
  constraints: Constraints,
  detectedVariables?: DetectedVariable[],
): ConstraintFormState {
  const variables: FormVariableRow[] = [];
  const detectedMap = new Map<string, DetectedVariable>();
  if (detectedVariables) {
    for (const dv of detectedVariables) {
      detectedMap.set(dv.name, dv);
    }
  }

  if (constraints.variables) {
    for (const [name, bound] of Object.entries(constraints.variables)) {
      const dv = detectedMap.get(name);
      const row: FormVariableRow = {
        name,
        min: bound.min !== undefined ? String(bound.min) : '',
        max: bound.max !== undefined ? String(bound.max) : '',
      };
      if (dv) {
        row.detectedInSource = dv.locationHint ?? dv.name;
      }
      variables.push(row);
    }
  }

  const sumConstraints: FormSumConstraintRow[] = (constraints.sum_constraints ?? []).map((sc) => ({
    variable: sc.variable,
    count_variable: sc.count_variable,
    max_total: String(sc.max_total),
  }));

  const productConstraints: FormProductConstraintRow[] = (constraints.product_constraints ?? []).map(
    (pc) => ({
      variables: [...pc.variables],
      max_product: String(pc.max_product),
    }),
  );

  const linearConstraints: FormLinearConstraintRow[] = (constraints.linear_constraints ?? []).map((lc) => {
    const coefficients: Record<string, string> = {};
    for (const [k, v] of Object.entries(lc.coefficients)) {
      coefficients[k] = String(v);
    }
    return {
      coefficients,
      bound: String(lc.bound),
    };
  });

  const state: ConstraintFormState = {
    variables,
    sumConstraints,
    productConstraints,
    linearConstraints,
  };

  if (constraints.time_limit_ms !== undefined) {
    state.timeLimitMs = String(constraints.time_limit_ms);
  }
  if (constraints.memory_limit_mb !== undefined) {
    state.memoryLimitMb = String(constraints.memory_limit_mb);
  }
  if (detectedVariables !== undefined) {
    state.detectedVariables = [...detectedVariables];
  }

  return state;
}

/**
 * Validates and converts ConstraintFormState into a protocol Constraints object.
 * Returns either ok with the Constraints object or ok: false with all accumulated validation errors.
 */
export function formToConstraints(formState: ConstraintFormState): FormToConstraintsResult {
  const errors: string[] = [];

  // 1. Variables
  const declaredVars = new Map<string, VariableBound>();
  const declaredNames = new Set<string>();

  for (const row of formState.variables) {
    const rawName = row.name;
    if (typeof rawName !== 'string' || !VARIABLE_NAME_REGEX.test(rawName)) {
      errors.push(
        `Invalid variable name "${rawName}": must match identifier pattern ^[A-Za-z_][A-Za-z0-9_]{0,63}(\\.size\\(\\))?$`,
      );
      continue;
    }

    if (declaredNames.has(rawName)) {
      errors.push(`Duplicate variable name: "${rawName}"`);
      continue;
    }
    declaredNames.add(rawName);

    const bound: VariableBound = {};
    let parsedMin: number | undefined;
    let parsedMax: number | undefined;

    if (row.min !== undefined && row.min.trim() !== '') {
      const minRes = parseNumericInput(row.min);
      if (minRes.ok) {
        parsedMin = minRes.value;
        bound.min = minRes.value;
      } else {
        errors.push(`Variable "${rawName}" min bound error: ${minRes.error}`);
      }
    }

    if (row.max !== undefined && row.max.trim() !== '') {
      const maxRes = parseNumericInput(row.max);
      if (maxRes.ok) {
        parsedMax = maxRes.value;
        bound.max = maxRes.value;
      } else {
        errors.push(`Variable "${rawName}" max bound error: ${maxRes.error}`);
      }
    }

    if (parsedMin !== undefined && parsedMax !== undefined && parsedMin > parsedMax) {
      errors.push(`Variable "${rawName}": min bound (${parsedMin}) cannot exceed max bound (${parsedMax})`);
    }

    declaredVars.set(rawName, bound);
  }

  // 2. Sum Constraints
  const outSumConstraints: { variable: string; count_variable: string; max_total: number }[] = [];

  for (const row of formState.sumConstraints) {
    const v = row.variable.trim();
    const cv = row.count_variable.trim();
    let valid = true;

    if (!VARIABLE_NAME_REGEX.test(v)) {
      errors.push(`Sum constraint has invalid variable identifier "${v}"`);
      valid = false;
    }
    if (!VARIABLE_NAME_REGEX.test(cv)) {
      errors.push(`Sum constraint has invalid count_variable identifier "${cv}"`);
      valid = false;
    }

    if (valid && v === cv) {
      errors.push(`Sum constraint variable and count_variable cannot be identical ("${v}")`);
      valid = false;
    }

    const vBound = declaredVars.get(v);
    if (!declaredNames.has(v)) {
      errors.push(`Sum constraint references undeclared variable "${v}"`);
      valid = false;
    } else if (vBound?.max === undefined) {
      errors.push(`Sum constraint variable "${v}" must have a max bound declared`);
      valid = false;
    }

    const cvBound = declaredVars.get(cv);
    if (!declaredNames.has(cv)) {
      errors.push(`Sum constraint references undeclared count_variable "${cv}"`);
      valid = false;
    } else if (cvBound?.max === undefined) {
      errors.push(`Sum constraint count_variable "${cv}" must have a max bound declared`);
      valid = false;
    }

    let parsedTotal: number | undefined;
    if (row.max_total.trim() === '') {
      errors.push('Sum constraint max_total cannot be empty');
      valid = false;
    } else {
      const totalRes = parseNumericInput(row.max_total);
      if (!totalRes.ok) {
        errors.push(`Sum constraint max_total error: ${totalRes.error}`);
        valid = false;
      } else if (totalRes.value < 0) {
        errors.push(`Sum constraint max_total must be non-negative, got ${totalRes.value}`);
        valid = false;
      } else {
        parsedTotal = totalRes.value;
      }
    }

    if (valid && parsedTotal !== undefined) {
      outSumConstraints.push({
        variable: v,
        count_variable: cv,
        max_total: parsedTotal,
      });
    }
  }

  // 3. Product Constraints
  const outProductConstraints: { variables: string[]; max_product: number }[] = [];

  for (const row of formState.productConstraints) {
    const rawVars = row.variables;
    const vars: string[] = Array.isArray(rawVars)
      ? rawVars.map((s) => s.trim()).filter((s) => s.length > 0)
      : rawVars
          .split(/[,\s]+/)
          .map((s) => s.trim())
          .filter((s) => s.length > 0);

    let valid = true;

    if (vars.length < 2) {
      errors.push(`Product constraint must include at least 2 variables, found ${vars.length}`);
      valid = false;
    }

    for (const v of vars) {
      if (!VARIABLE_NAME_REGEX.test(v)) {
        errors.push(`Product constraint has invalid variable identifier "${v}"`);
        valid = false;
      } else if (!declaredNames.has(v)) {
        errors.push(`Product constraint references undeclared variable "${v}"`);
        valid = false;
      }
    }

    let parsedProduct: number | undefined;
    if (row.max_product.trim() === '') {
      errors.push('Product constraint max_product cannot be empty');
      valid = false;
    } else {
      const prodRes = parseNumericInput(row.max_product);
      if (!prodRes.ok) {
        errors.push(`Product constraint max_product error: ${prodRes.error}`);
        valid = false;
      } else if (prodRes.value < 0) {
        errors.push(`Product constraint max_product must be non-negative, got ${prodRes.value}`);
        valid = false;
      } else {
        parsedProduct = prodRes.value;
      }
    }

    if (valid && parsedProduct !== undefined) {
      outProductConstraints.push({
        variables: vars,
        max_product: parsedProduct,
      });
    }
  }

  // 4. Linear Constraints
  const outLinearConstraints: { coefficients: Record<string, number>; bound: number }[] = [];

  for (const row of formState.linearConstraints) {
    const coeffEntries = Object.entries(row.coefficients);
    let valid = true;

    if (coeffEntries.length < 1) {
      errors.push('Linear constraint must include at least one coefficient');
      valid = false;
    }

    const parsedCoeffs: Record<string, number> = {};
    for (const [v, cStr] of coeffEntries) {
      const vTrim = v.trim();
      if (!VARIABLE_NAME_REGEX.test(vTrim)) {
        errors.push(`Linear constraint has invalid variable identifier "${v}"`);
        valid = false;
      }
      const cRes = parseNumericInput(cStr);
      if (!cRes.ok) {
        errors.push(`Linear constraint coefficient for "${v}" error: ${cRes.error}`);
        valid = false;
      } else {
        parsedCoeffs[vTrim] = cRes.value;
      }
    }

    let parsedBound: number | undefined;
    if (row.bound.trim() === '') {
      errors.push('Linear constraint bound cannot be empty');
      valid = false;
    } else {
      const bRes = parseNumericInput(row.bound);
      if (!bRes.ok) {
        errors.push(`Linear constraint bound error: ${bRes.error}`);
        valid = false;
      } else {
        parsedBound = bRes.value;
      }
    }

    if (valid && parsedBound !== undefined) {
      outLinearConstraints.push({
        coefficients: parsedCoeffs,
        bound: parsedBound,
      });
    }
  }

  // 5. Time Limit
  let outTimeLimitMs: number | undefined;
  if (formState.timeLimitMs !== undefined && formState.timeLimitMs.trim() !== '') {
    const tRes = parseNumericInput(formState.timeLimitMs);
    if (!tRes.ok) {
      errors.push(`time_limit_ms error: ${tRes.error}`);
    } else if (tRes.value < TIME_LIMIT_MS_MIN || tRes.value > TIME_LIMIT_MS_MAX) {
      errors.push(
        `time_limit_ms must be an integer between ${TIME_LIMIT_MS_MIN} and ${TIME_LIMIT_MS_MAX} (got ${tRes.value})`,
      );
    } else {
      outTimeLimitMs = tRes.value;
    }
  }

  // 6. Memory Limit
  let outMemoryLimitMb: number | undefined;
  if (formState.memoryLimitMb !== undefined && formState.memoryLimitMb.trim() !== '') {
    const mRes = parseNumericInput(formState.memoryLimitMb);
    if (!mRes.ok) {
      errors.push(`memory_limit_mb error: ${mRes.error}`);
    } else if (mRes.value < MEMORY_LIMIT_MB_MIN || mRes.value > MEMORY_LIMIT_MB_MAX) {
      errors.push(
        `memory_limit_mb must be an integer between ${MEMORY_LIMIT_MB_MIN} and ${MEMORY_LIMIT_MB_MAX} (got ${mRes.value})`,
      );
    } else {
      outMemoryLimitMb = mRes.value;
    }
  }

  if (errors.length > 0) {
    return { ok: false, errors };
  }

  const constraints: Constraints = {};
  if (declaredVars.size > 0) {
    const varsObj: Record<string, VariableBound> = {};
    for (const [name, b] of declaredVars.entries()) {
      varsObj[name] = b;
    }
    constraints.variables = varsObj;
  }
  if (outSumConstraints.length > 0) {
    constraints.sum_constraints = outSumConstraints;
  }
  if (outProductConstraints.length > 0) {
    constraints.product_constraints = outProductConstraints;
  }
  if (outLinearConstraints.length > 0) {
    constraints.linear_constraints = outLinearConstraints;
  }
  if (outTimeLimitMs !== undefined) {
    constraints.time_limit_ms = outTimeLimitMs;
  }
  if (outMemoryLimitMb !== undefined) {
    constraints.memory_limit_mb = outMemoryLimitMb;
  }

  return { ok: true, constraints };
}
