import assert from 'node:assert/strict';
import { describe, it } from 'node:test';

import { validateConstraintFormMessage } from '../../src/models/constraintFormMessages';

describe('constraintFormMessages - Message Validation', () => {
  it('accepts valid ready message', () => {
    const res = validateConstraintFormMessage({ type: 'ready' });
    assert.equal(res.ok, true);
    assert.equal(res.message.type, 'ready');
  });

  it('accepts valid validate message', () => {
    const res = validateConstraintFormMessage({
      type: 'validate',
      state: {
        variables: [{ name: 'n', min: '1', max: '100' }],
        sumConstraints: [],
        productConstraints: [],
        linearConstraints: [],
      },
    });
    assert.equal(res.ok, true);
    assert.equal(res.message.type, 'validate');
  });

  it('accepts valid save message with andAnalyze flag', () => {
    const res = validateConstraintFormMessage({
      type: 'save',
      state: {
        variables: [{ name: 'n', min: '1', max: '100' }],
        sumConstraints: [],
        productConstraints: [],
        linearConstraints: [],
      },
      andAnalyze: true,
    });
    assert.equal(res.ok, true);
    assert.equal(res.message.type, 'save');
    assert.equal(res.message.andAnalyze, true);
  });

  it('rejects non-objects and nulls', () => {
    assert.equal(validateConstraintFormMessage(null).ok, false);
    assert.equal(validateConstraintFormMessage(undefined).ok, false);
    assert.equal(validateConstraintFormMessage('hello').ok, false);
    assert.equal(validateConstraintFormMessage([1, 2, 3]).ok, false);
  });

  it('rejects unknown message types', () => {
    const res = validateConstraintFormMessage({ type: 'unknown_cmd' });
    assert.equal(res.ok, false);
    assert.match(res.error, /Unknown form message type/);
  });

  it('rejects validate and save messages with malformed state', () => {
    const resNoState = validateConstraintFormMessage({ type: 'validate' });
    assert.equal(resNoState.ok, false);

    const resInvalidState = validateConstraintFormMessage({
      type: 'save',
      state: { variables: 'not_an_array' },
    });
    assert.equal(resInvalidState.ok, false);
  });
});
