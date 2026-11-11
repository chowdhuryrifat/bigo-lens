import assert from 'node:assert/strict';
import { describe, it } from 'node:test';

import {
  decodePointerToken,
  findJsonPointerOffset,
  findJsonPointerRange,
  offsetToPosition,
  parsePointer,
} from '../../src/services/jsonPointer';

describe('JSON pointer mapping', () => {
  it('decodes RFC 6901 tokens with escaped characters', () => {
    assert.equal(decodePointerToken('normal'), 'normal');
    assert.equal(decodePointerToken('a~1b'), 'a/b');
    assert.equal(decodePointerToken('a~0b'), 'a~b');
    assert.equal(decodePointerToken('~01'), '~1');
    assert.equal(decodePointerToken('v.size()'), 'v.size()');
  });

  it('splits pointers into decoded segments', () => {
    assert.deepEqual(parsePointer('/variables/n'), ['variables', 'n']);
    assert.deepEqual(parsePointer('/variables/v.size()'), ['variables', 'v.size()']);
    assert.deepEqual(parsePointer('/sum_constraints/0/max_total'), ['sum_constraints', '0', 'max_total']);
    assert.deepEqual(parsePointer('/'), []);
    assert.deepEqual(parsePointer(''), []);
  });

  it('converts character offsets to 0-based lines and columns', () => {
    const text = 'line 0\nline 1\nline 2';
    assert.deepEqual(offsetToPosition(text, 0), { line: 0, character: 0 });
    assert.deepEqual(offsetToPosition(text, 5), { line: 0, character: 5 });
    assert.deepEqual(offsetToPosition(text, 7), { line: 1, character: 0 });
    assert.deepEqual(offsetToPosition(text, 14), { line: 2, character: 0 });
  });

  it('underlines the variable entry for an inverted range', () => {
    const json = `{
  "variables": {
    "n": {
      "min": 10,
      "max": 1
    }
  }
}`;
    const offset = findJsonPointerOffset(json, '/variables/n');
    assert.ok(offset);
    const sub = json.slice(offset.startOffset, offset.endOffset);
    assert.equal(sub, '"n"');

    const range = findJsonPointerRange(json, '/variables/n');
    assert.ok(range);
    assert.equal(range.start.line, 2);
    assert.equal(range.start.character, 4);
    assert.equal(range.end.line, 2);
    assert.equal(range.end.character, 7);
  });

  it('locates complex and dotted keys like v.size()', () => {
    const json = `{
  "variables": {
    "v.size()": {
      "min": 1,
      "max": 100
    }
  }
}`;
    const offset = findJsonPointerOffset(json, '/variables/v.size()');
    assert.ok(offset);
    assert.equal(json.slice(offset.startOffset, offset.endOffset), '"v.size()"');
  });

  it('locates keys containing escaped slashes and tildes', () => {
    const json = `{
  "variables": {
    "a/b": { "max": 10 },
    "a~b": { "max": 20 }
  }
}`;
    const offsetSlash = findJsonPointerOffset(json, '/variables/a~1b');
    assert.ok(offsetSlash);
    assert.equal(json.slice(offsetSlash.startOffset, offsetSlash.endOffset), '"a/b"');

    const offsetTilde = findJsonPointerOffset(json, '/variables/a~0b');
    assert.ok(offsetTilde);
    assert.equal(json.slice(offsetTilde.startOffset, offsetTilde.endOffset), '"a~b"');
  });

  it('locates array elements and properties inside arrays', () => {
    const json = `{
  "sum_constraints": [
    {
      "variable": "n",
      "count_variable": "t",
      "max_total": 200000
    }
  ]
}`;
    const itemOffset = findJsonPointerOffset(json, '/sum_constraints/0');
    assert.ok(itemOffset);

    const propOffset = findJsonPointerOffset(json, '/sum_constraints/0/max_total');
    assert.ok(propOffset);
    assert.equal(json.slice(propOffset.startOffset, propOffset.endOffset), '"max_total"');
  });

  it('returns undefined for nonexistent pointers', () => {
    const json = '{"variables": {}}';
    assert.equal(findJsonPointerOffset(json, '/variables/nonexistent'), undefined);
    assert.equal(findJsonPointerOffset(json, '/other/0'), undefined);
  });
});
