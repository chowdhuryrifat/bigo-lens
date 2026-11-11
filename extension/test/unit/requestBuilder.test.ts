import assert from 'node:assert/strict';
import { describe, it } from 'node:test';

import { PROTOCOL_VERSION } from '../../src/models/protocol';
import { analyzeRequest, capabilitiesRequest, newRequestId } from '../../src/services/requestBuilder';

describe('requestBuilder', () => {
  it('builds a capabilities request without a source', () => {
    const request = capabilitiesRequest('cap-1');
    assert.deepEqual(request, {
      schema_version: PROTOCOL_VERSION,
      request_id: 'cap-1',
      action: 'capabilities',
    });
  });

  it('builds an analyze request and omits absent constraints', () => {
    const request = analyzeRequest(
      { displayPath: 'src/a.cpp', content: 'int main(){}', constraints: undefined, cppStandard: 'c++17' },
      'an-1',
    );
    assert.equal(request.action, 'analyze');
    assert.deepEqual(request.source, { path: 'src/a.cpp', language: 'cpp', content: 'int main(){}' });
    assert.deepEqual(request.options, { cpp_standard: 'c++17' });
    assert.equal('constraints' in request, false);
  });

  it('passes constraints through unchanged', () => {
    const constraints = { variables: { n: { min: 1, max: 10 } }, time_limit_ms: 1000 };
    const request = analyzeRequest({ displayPath: 'a.cpp', content: '', constraints, cppStandard: 'c++20' });
    assert.deepEqual(request.constraints, constraints);
  });

  it('builds an analyze request with language python', () => {
    const request = analyzeRequest(
      {
        displayPath: 'solution.py',
        content: 'for i in range(n): pass',
        constraints: undefined,
        language: 'python',
      },
      'an-py',
    );
    assert.equal(request.action, 'analyze');
    assert.deepEqual(request.source, {
      path: 'solution.py',
      language: 'python',
      content: 'for i in range(n): pass',
    });
    assert.equal('options' in request, false);
  });

  it('generates request ids accepted by the protocol', () => {
    const id = newRequestId();
    assert.match(id, /^[A-Za-z0-9_.:-]{1,128}$/);
    assert.notEqual(id, newRequestId());
  });
});
