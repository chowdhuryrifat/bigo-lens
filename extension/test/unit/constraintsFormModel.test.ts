import assert from 'node:assert/strict';
import { describe, it } from 'node:test';

import type { Constraints } from '../../src/models/protocol';
import { CONSTRAINTS_TEMPLATE } from '../../src/services/constraintsStore';
import {
  constraintsToForm,
  formToConstraints,
  parseNumericInput,
  validateVariableName,
  type ConstraintFormState,
  type DetectedVariable,
} from '../../src/services/constraintsFormModel';

describe('constraintsFormModel - parseNumericInput', () => {
  it('parses standard integer representations and trims whitespace', () => {
    assert.deepEqual(parseNumericInput('0'), { ok: true, value: 0 });
    assert.deepEqual(parseNumericInput('  42  '), { ok: true, value: 42 });
    assert.deepEqual(parseNumericInput('-100'), { ok: true, value: -100 });
    assert.deepEqual(parseNumericInput('200000'), { ok: true, value: 200000 });
  });

  it('parses scientific notation producing integers', () => {
    assert.deepEqual(parseNumericInput('2e5'), { ok: true, value: 200000 });
    assert.deepEqual(parseNumericInput('1e4'), { ok: true, value: 10000 });
    assert.deepEqual(parseNumericInput('2.5e3'), { ok: true, value: 2500 });
    assert.deepEqual(parseNumericInput('1E6'), { ok: true, value: 1000000 });
  });

  it('parses power multiplication expressions', () => {
    assert.deepEqual(parseNumericInput('2*10^5'), { ok: true, value: 200000 });
    assert.deepEqual(parseNumericInput('10^5'), { ok: true, value: 100000 });
    assert.deepEqual(parseNumericInput('3 * 10^4'), { ok: true, value: 30000 });
    assert.deepEqual(parseNumericInput('10**6'), { ok: true, value: 1000000 });
    assert.deepEqual(parseNumericInput('2.5 * 10^3'), { ok: true, value: 2500 });
  });

  it('rejects non-integers, floats, and fractions', () => {
    const resFloat = parseNumericInput('3.14');
    assert.equal(resFloat.ok, false);
    assert.match(resFloat.error, /must be an integer/);

    const resSciNonInt = parseNumericInput('1.23e1');
    assert.equal(resSciNonInt.ok, false);
    assert.match(resSciNonInt.error, /must be an integer/);
  });

  it('rejects empty strings and malformed inputs', () => {
    assert.equal(parseNumericInput('').ok, false);
    assert.equal(parseNumericInput('   ').ok, false);
    assert.equal(parseNumericInput('abc').ok, false);
    assert.equal(parseNumericInput('10^').ok, false);
    assert.equal(parseNumericInput('2 * foo').ok, false);
  });

  it('rejects numbers exceeding magnitude 10^18', () => {
    const outOfRange = parseNumericInput('1e19');
    assert.equal(outOfRange.ok, false);
    assert.match(outOfRange.error, /maximum allowed magnitude/);
  });
});

describe('constraintsFormModel - validateVariableName', () => {
  it('accepts valid C++ identifiers and container size expressions', () => {
    assert.equal(validateVariableName('n').ok, true);
    assert.equal(validateVariableName('t').ok, true);
    assert.equal(validateVariableName('test_case').ok, true);
    assert.equal(validateVariableName('v.size()').ok, true);
    assert.equal(validateVariableName('_arr_len').ok, true);
  });

  it('rejects invalid variable identifiers', () => {
    assert.equal(validateVariableName('').ok, false);
    assert.equal(validateVariableName('123n').ok, false);
    assert.equal(validateVariableName('n-m').ok, false);
    assert.equal(validateVariableName('v.length()').ok, false);
    assert.equal(validateVariableName('v.size().other').ok, false);
    assert.equal(validateVariableName('a'.repeat(65)).ok, false);
  });
});

describe('constraintsFormModel - round trip conversions and lossless serialization', () => {
  it('preserves CONSTRAINTS_TEMPLATE losslessly', () => {
    const formState = constraintsToForm(CONSTRAINTS_TEMPLATE);
    const converted = formToConstraints(formState);
    assert.equal(converted.ok, true);
    assert.deepEqual(converted.constraints, CONSTRAINTS_TEMPLATE);
  });

  it('handles empty constraints object losslessly', () => {
    const emptyConstraints: Constraints = {};
    const formState = constraintsToForm(emptyConstraints);
    const converted = formToConstraints(formState);
    assert.equal(converted.ok, true);
    assert.deepEqual(converted.constraints, {});
  });

  it('handles full constraints with variables, sum, product, linear, and limits', () => {
    const complexConstraints: Constraints = {
      variables: {
        n: { min: 1, max: 200000 },
        m: { min: 0, max: 5000 },
        t: { min: 1, max: 10 },
      },
      sum_constraints: [{ variable: 'n', count_variable: 't', max_total: 200000 }],
      product_constraints: [{ variables: ['n', 'm'], max_product: 1000000 }],
      linear_constraints: [{ coefficients: { n: 1, m: 2 }, bound: 300000 }],
      time_limit_ms: 2000,
      memory_limit_mb: 256,
    };

    const formState = constraintsToForm(complexConstraints);
    const converted = formToConstraints(formState);
    assert.equal(converted.ok, true);
    assert.deepEqual(converted.constraints, complexConstraints);
  });

  it('preserves detected variables annotations in form state', () => {
    const constraints: Constraints = {
      variables: {
        n: { min: 1, max: 100 },
      },
    };
    const detected: DetectedVariable[] = [
      { name: 'n', status: 'used', locationHint: 'loop at line 12' },
      { name: 'm', status: 'missing_bound', locationHint: 'inner loop at line 15' },
    ];

    const formState = constraintsToForm(constraints, detected);
    assert.equal(formState.variables.length, 1);
    const firstVar = formState.variables[0];
    assert.ok(firstVar);
    assert.equal(firstVar.name, 'n');
    assert.equal(firstVar.detectedInSource, 'loop at line 12');
    assert.ok(formState.detectedVariables);
    assert.equal(formState.detectedVariables.length, 2);
  });
});

describe('constraintsFormModel - validation errors', () => {
  it('rejects inverted bounds where min > max', () => {
    const state: ConstraintFormState = {
      variables: [{ name: 'n', min: '100', max: '50' }],
      sumConstraints: [],
      productConstraints: [],
      linearConstraints: [],
    };
    const res = formToConstraints(state);
    assert.equal(res.ok, false);
    assert.ok(res.errors.some((e) => e.includes('min bound (100) cannot exceed max bound (50)')));
  });

  it('rejects duplicate variable names', () => {
    const state: ConstraintFormState = {
      variables: [
        { name: 'n', min: '1', max: '10' },
        { name: 'n', min: '5', max: '20' },
      ],
      sumConstraints: [],
      productConstraints: [],
      linearConstraints: [],
    };
    const res = formToConstraints(state);
    assert.equal(res.ok, false);
    assert.ok(res.errors.some((e) => e.includes('Duplicate variable name: "n"')));
  });

  it('rejects sum constraints where variable and count_variable are identical', () => {
    const state: ConstraintFormState = {
      variables: [{ name: 'n', min: '1', max: '100' }],
      sumConstraints: [{ variable: 'n', count_variable: 'n', max_total: '1000' }],
      productConstraints: [],
      linearConstraints: [],
    };
    const res = formToConstraints(state);
    assert.equal(res.ok, false);
    assert.ok(res.errors.some((e) => e.includes('cannot be identical')));
  });

  it('rejects sum constraints where variables lack max bounds or are undeclared', () => {
    const state: ConstraintFormState = {
      variables: [
        { name: 'n', min: '1' }, // no max
        { name: 't', min: '1', max: '10' },
      ],
      sumConstraints: [
        { variable: 'n', count_variable: 't', max_total: '1000' },
        { variable: 'unknown_var', count_variable: 't', max_total: '500' },
      ],
      productConstraints: [],
      linearConstraints: [],
    };
    const res = formToConstraints(state);
    assert.equal(res.ok, false);
    assert.ok(res.errors.some((e) => e.includes('must have a max bound declared')));
    assert.ok(res.errors.some((e) => e.includes('references undeclared variable "unknown_var"')));
  });

  it('rejects product constraints with fewer than 2 variables', () => {
    const state: ConstraintFormState = {
      variables: [{ name: 'n', min: '1', max: '100' }],
      sumConstraints: [],
      productConstraints: [{ variables: ['n'], max_product: '500' }],
      linearConstraints: [],
    };
    const res = formToConstraints(state);
    assert.equal(res.ok, false);
    assert.ok(res.errors.some((e) => e.includes('must include at least 2 variables')));
  });

  it('rejects out of bounds time_limit_ms and memory_limit_mb', () => {
    const state: ConstraintFormState = {
      variables: [],
      sumConstraints: [],
      productConstraints: [],
      linearConstraints: [],
      timeLimitMs: '0',
      memoryLimitMb: '2000000',
    };
    const res = formToConstraints(state);
    assert.equal(res.ok, false);
    assert.ok(res.errors.some((e) => e.includes('time_limit_ms must be an integer between')));
    assert.ok(res.errors.some((e) => e.includes('memory_limit_mb must be an integer between')));
  });
});
