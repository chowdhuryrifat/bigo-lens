import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { after, describe, it } from 'node:test';

import {
  CONSTRAINTS_TEMPLATE,
  constraintsPathFor,
  createConstraintsTemplate,
  loadConstraints,
} from '../../src/services/constraintsStore';
import { SchemaValidator } from '../../src/services/schemaValidator';
import { schemaDirectory } from '../helpers';

const validator = new SchemaValidator(schemaDirectory);
const dir = mkdtempSync(join(tmpdir(), 'bigolens-test-'));
after(() => {
  rmSync(dir, { recursive: true, force: true });
});

describe('constraints sidecar', () => {
  it('derives the sidecar path from the source path', () => {
    assert.equal(constraintsPathFor(join(dir, 'a.cpp')), join(dir, 'a.cpp.bigolens.json'));
  });

  it('reports a missing file as missing, not as an error', async () => {
    assert.deepEqual(await loadConstraints(join(dir, 'none.json'), validator), { kind: 'missing' });
  });

  it('creates a valid template and never overwrites', async () => {
    const path = join(dir, 'template.cpp.bigolens.json');
    assert.equal(await createConstraintsTemplate(path), true);
    writeFileSync(path, '{"variables": {"n": {"max": 5}}}');
    assert.equal(await createConstraintsTemplate(path), false);
    assert.match(readFileSync(path, 'utf8'), /"max": 5/);
    assert.deepEqual(validator.validateConstraints(CONSTRAINTS_TEMPLATE).errors, []);
  });

  it('loads valid constraints', async () => {
    const path = join(dir, 'ok.json');
    writeFileSync(path, JSON.stringify({ variables: { n: { min: 1, max: 100 } }, time_limit_ms: 1000 }));
    const loaded = await loadConstraints(path, validator);
    assert.equal(loaded.kind, 'ok');
  });

  it('reports malformed JSON and schema violations', async () => {
    const malformed = join(dir, 'malformed.json');
    writeFileSync(malformed, '{ "variables": ');
    assert.equal((await loadConstraints(malformed, validator)).kind, 'invalid');
    const wrong = join(dir, 'wrong.json');
    writeFileSync(wrong, JSON.stringify({ variables: { n: { max: 'big' } } }));
    const loaded = await loadConstraints(wrong, validator);
    assert.ok(loaded.kind === 'invalid' && loaded.errors.length > 0);
  });
});
