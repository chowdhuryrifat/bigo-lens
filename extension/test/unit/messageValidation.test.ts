import assert from 'node:assert/strict';
import { describe, it } from 'node:test';

import { isWebviewIncomingMessage, validateWebviewMessage } from '../../src/models/webview';

describe('Webview Message Validation', () => {
  it('accepts a valid ready message', () => {
    const result = validateWebviewMessage({ type: 'ready' });
    assert.equal(result.ok, true);
    assert.deepEqual(result.message, { type: 'ready' });
  });

  it('accepts a valid runBenchmark message', () => {
    const result = validateWebviewMessage({ type: 'runBenchmark' });
    assert.equal(result.ok, true);
    assert.deepEqual(result.message, { type: 'runBenchmark' });
  });

  it('accepts a valid navigate message with line only', () => {
    const result = validateWebviewMessage({
      type: 'navigate',
      location: { line: 42 },
    });
    assert.equal(result.ok, true);
    assert.deepEqual(result.message, {
      type: 'navigate',
      location: { line: 42 },
    });
  });

  it('accepts a valid navigate message with file, line, and column', () => {
    const result = validateWebviewMessage({
      type: 'navigate',
      location: {
        file: 'src/solution.cpp',
        line: 15,
        column: 8,
      },
    });
    assert.equal(result.ok, true);
    assert.deepEqual(result.message, {
      type: 'navigate',
      location: {
        file: 'src/solution.cpp',
        line: 15,
        column: 8,
      },
    });
  });

  it('rejects unknown message types', () => {
    assert.equal(validateWebviewMessage({ type: 'unknown_type' }).ok, false);
    assert.equal(validateWebviewMessage({ type: 'execCommand' }).ok, false);
    assert.equal(validateWebviewMessage({ type: '<script>alert(1)</script>' }).ok, false);
    assert.equal(validateWebviewMessage({ type: '__proto__' }).ok, false);
  });

  it('rejects non-object or nullish inputs', () => {
    assert.equal(validateWebviewMessage(null).ok, false);
    assert.equal(validateWebviewMessage(undefined).ok, false);
    assert.equal(validateWebviewMessage('ready').ok, false);
    assert.equal(validateWebviewMessage(12345).ok, false);
    assert.equal(validateWebviewMessage(true).ok, false);
    assert.equal(validateWebviewMessage([]).ok, false);
    assert.equal(validateWebviewMessage([{ type: 'ready' }]).ok, false);
  });

  it('rejects objects missing a valid string type property', () => {
    assert.equal(validateWebviewMessage({}).ok, false);
    assert.equal(validateWebviewMessage({ type: 123 }).ok, false);
    assert.equal(validateWebviewMessage({ type: '' }).ok, false);
    assert.equal(validateWebviewMessage({ type: '   ' }).ok, false);
    assert.equal(validateWebviewMessage({ type: null }).ok, false);
  });

  it('rejects navigate message with missing or malformed location', () => {
    assert.equal(validateWebviewMessage({ type: 'navigate' }).ok, false);
    assert.equal(validateWebviewMessage({ type: 'navigate', location: null }).ok, false);
    assert.equal(validateWebviewMessage({ type: 'navigate', location: 'line 10' }).ok, false);
    assert.equal(validateWebviewMessage({ type: 'navigate', location: [] }).ok, false);
    assert.equal(validateWebviewMessage({ type: 'navigate', location: {} }).ok, false);
  });

  it('rejects invalid line numbers in navigate message', () => {
    assert.equal(validateWebviewMessage({ type: 'navigate', location: { line: 0 } }).ok, false);
    assert.equal(validateWebviewMessage({ type: 'navigate', location: { line: -5 } }).ok, false);
    assert.equal(validateWebviewMessage({ type: 'navigate', location: { line: 3.14 } }).ok, false);
    assert.equal(validateWebviewMessage({ type: 'navigate', location: { line: '42' } }).ok, false);
    assert.equal(validateWebviewMessage({ type: 'navigate', location: { line: NaN } }).ok, false);
    assert.equal(validateWebviewMessage({ type: 'navigate', location: { line: Infinity } }).ok, false);
  });

  it('rejects invalid column numbers in navigate message', () => {
    assert.equal(validateWebviewMessage({ type: 'navigate', location: { line: 10, column: 0 } }).ok, false);
    assert.equal(validateWebviewMessage({ type: 'navigate', location: { line: 10, column: -1 } }).ok, false);
    assert.equal(validateWebviewMessage({ type: 'navigate', location: { line: 10, column: 1.5 } }).ok, false);
    assert.equal(validateWebviewMessage({ type: 'navigate', location: { line: 10, column: '8' } }).ok, false);
  });

  it('rejects invalid file values in navigate message', () => {
    assert.equal(validateWebviewMessage({ type: 'navigate', location: { line: 10, file: '' } }).ok, false);
    assert.equal(validateWebviewMessage({ type: 'navigate', location: { line: 10, file: '   ' } }).ok, false);
    assert.equal(validateWebviewMessage({ type: 'navigate', location: { line: 10, file: 123 } }).ok, false);
  });

  it('isWebviewIncomingMessage behaves consistently with validateWebviewMessage', () => {
    assert.equal(isWebviewIncomingMessage({ type: 'ready' }), true);
    assert.equal(isWebviewIncomingMessage({ type: 'navigate', location: { line: 1 } }), true);
    assert.equal(isWebviewIncomingMessage({ type: 'bad' }), false);
    assert.equal(isWebviewIncomingMessage(null), false);
  });
});
