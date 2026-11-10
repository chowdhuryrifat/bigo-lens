import assert from 'node:assert/strict';
import { describe, it } from 'node:test';

import { EngineClient, EngineError } from '../../src/services/engineClient';
import { capabilitiesRequest } from '../../src/services/requestBuilder';
import { SchemaValidator } from '../../src/services/schemaValidator';
import { fakeEnginePath, schemaDirectory } from '../helpers';

const validator = new SchemaValidator(schemaDirectory);

function clientFor(mode: string, timeoutMs = 5000): EngineClient {
  process.env.FAKE_ENGINE_MODE = mode;
  return new EngineClient({
    enginePath: process.execPath,
    launcherArgs: [fakeEnginePath],
    timeoutMs,
    validator,
  });
}

async function expectEngineError(promise: Promise<unknown>, kind: EngineError['kind']): Promise<void> {
  await assert.rejects(promise, (err: unknown) => err instanceof EngineError && err.kind === kind);
}

describe('EngineClient', () => {
  it('returns a validated response', async () => {
    const response = await clientFor('ok').send(capabilitiesRequest('req-1'));
    assert.equal(response.status, 'ok');
    assert.equal(response.request_id, 'req-1');
  });

  it('rejects non-JSON output', async () => {
    await expectEngineError(clientFor('garbage').send(capabilitiesRequest()), 'output');
  });

  it('rejects responses that violate the schema', async () => {
    await expectEngineError(clientFor('schema-violation').send(capabilitiesRequest()), 'protocol');
  });

  it('rejects responses for a different request', async () => {
    await expectEngineError(clientFor('wrong-id').send(capabilitiesRequest()), 'protocol');
  });

  it('reports a non-zero exit status with stderr', async () => {
    await assert.rejects(clientFor('exit-1').send(capabilitiesRequest()), (err: unknown) => {
      return err instanceof EngineError && err.kind === 'exit' && err.stderr.includes('simulated failure');
    });
  });

  it('kills an engine that exceeds the timeout', async () => {
    await expectEngineError(clientFor('hang', 300).send(capabilitiesRequest()), 'timeout');
  });

  it('supports cancellation', async () => {
    const controller = new AbortController();
    const pending = clientFor('hang').send(capabilitiesRequest(), controller.signal);
    setTimeout(() => {
      controller.abort();
    }, 100);
    await expectEngineError(pending, 'cancelled');
  });

  it('reports a missing executable', async () => {
    const client = new EngineClient({
      enginePath: '/definitely/not/here/bigolens-engine',
      timeoutMs: 2000,
      validator,
    });
    await expectEngineError(client.send(capabilitiesRequest()), 'spawn');
  });

  it('refuses to send an invalid request', async () => {
    const bad = { ...capabilitiesRequest(), request_id: 'has spaces' };
    await expectEngineError(clientFor('ok').send(bad), 'protocol');
  });
});
