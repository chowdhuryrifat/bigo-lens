import assert from 'node:assert/strict';
import { describe, it } from 'node:test';

import { formatBytes, formatCount } from '../../src/services/format';

describe('format', () => {
  it('formats counts without inventing precision', () => {
    assert.equal(formatCount(null), 'unknown');
    assert.equal(formatCount(1000), '1000');
    assert.equal(formatCount(4e10), '4.000e10');
    assert.equal(formatCount(Number.POSITIVE_INFINITY), 'overflow');
  });

  it('formats bytes with binary units', () => {
    assert.equal(formatBytes(null), 'unknown');
    assert.equal(formatBytes(512), '512 B');
    assert.equal(formatBytes(800020), '781.27 KiB');
    assert.equal(formatBytes(256 * 1024 * 1024), '256.00 MiB');
  });
});
