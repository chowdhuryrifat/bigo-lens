import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, it } from 'node:test';

import { SchemaValidator } from '../../src/services/schemaValidator';
import { analyzeRequest, capabilitiesRequest } from '../../src/services/requestBuilder';
import { examplesDirectory, schemaDirectory } from '../helpers';

const validator = new SchemaValidator(schemaDirectory);

describe('SchemaValidator', () => {
  it('accepts the request builders output', () => {
    assert.deepEqual(validator.validateRequest(capabilitiesRequest('id-1')).errors, []);
    const analyze = analyzeRequest(
      {
        displayPath: 'a.cpp',
        content: 'int main(){}',
        constraints: { variables: { n: { max: 5 } } },
        cppStandard: 'c++17',
      },
      'id-2',
    );
    assert.deepEqual(validator.validateRequest(analyze).errors, []);

    for (const lang of ['python', 'java', 'rust', 'javascript']) {
      const multiLangReq = analyzeRequest(
        {
          displayPath: `solution.${lang}`,
          content: 'code',
          constraints: undefined,
          language: lang,
        },
        `id-${lang}`,
      );
      assert.deepEqual(validator.validateRequest(multiLangReq).errors, []);
    }
  });

  it('rejects requests the engine would reject', () => {
    assert.ok(
      validator.validateRequest({ schema_version: '1.0', request_id: 'x', action: 'analyze' }).errors.length >
        0,
    );
    assert.ok(validator.validateRequest({ ...capabilitiesRequest('x'), extra: true }).errors.length > 0);
    assert.ok(validator.validateRequest({ ...capabilitiesRequest('bad id') }).errors.length > 0);
  });

  it('validates constraint documents', () => {
    assert.ok(validator.validateConstraints({ variables: { n: { min: 1, max: 200000 } } }).value);
    assert.ok(validator.validateConstraints({ variables: { 'not valid': { max: 1 } } }).errors.length > 0);
    assert.ok(validator.validateConstraints({ time_limit_ms: 0 }).errors.length > 0);
    assert.ok(validator.validateConstraints({ variables: { n: { max: 1e19 } } }).errors.length > 0);
  });

  it('accepts every example response produced by the real engine', () => {
    const responses = readdirSync(examplesDirectory).filter((n) => n.endsWith('.response.json'));
    assert.ok(responses.length > 0, 'no example responses found');
    for (const name of responses) {
      const document: unknown = JSON.parse(readFileSync(join(examplesDirectory, name), 'utf8'));
      assert.deepEqual(validator.validateResponse(document).errors, [], name);
    }
  });

  it('rejects an ok response that carries errors', () => {
    const document = {
      schema_version: '1.0',
      request_id: 'x',
      status: 'ok',
      engine: { name: 'e', version: '0', protocol_version: '1.0', frontend: null },
      results: null,
      diagnostics: [],
      errors: [{ code: 'internal_error', message: 'boom' }],
    };
    assert.ok(validator.validateResponse(document).errors.length > 0);
  });
});
