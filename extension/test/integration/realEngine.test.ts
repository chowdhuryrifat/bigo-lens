// Runs the real native engine when one can be located (BIGOLENS_ENGINE_PATH or
// a development build); otherwise the tests are skipped with a reason.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, it } from 'node:test';

import { isAnalyzeResults } from '../../src/models/protocol';
import { constraintsPathFor, loadConstraints } from '../../src/services/constraintsStore';
import { EngineClient } from '../../src/services/engineClient';
import { locateEngine } from '../../src/services/engineLocator';
import { formatCount, statusLabel, summarizeFinding } from '../../src/services/format';
import { analyzeRequest, capabilitiesRequest } from '../../src/services/requestBuilder';
import { SchemaValidator } from '../../src/services/schemaValidator';
import { extensionRoot, fixturesDirectory, schemaDirectory } from '../helpers';

const located = locateEngine({
  configuredPath: '',
  environmentPath: process.env.BIGOLENS_ENGINE_PATH,
  extensionRoot,
  platform: process.platform,
});
const skip = located.path
  ? false
  : `native engine not found (${located.problem ?? 'build it or set BIGOLENS_ENGINE_PATH'})`;
const client = located.path
  ? new EngineClient({
      enginePath: located.path,
      timeoutMs: 60000,
      validator: new SchemaValidator(schemaDirectory),
    })
  : undefined;

describe('real engine', { skip }, () => {
  it('answers a capabilities request', async () => {
    const response = await client!.send(capabilitiesRequest());
    assert.equal(response.status, 'ok');
    assert.equal(response.engine.protocol_version, '1.0');
  });

  it('analyzes a sum-constrained fixture end to end using sidecar constraints', async (t) => {
    const sourcePath = join(fixturesDirectory, '09_sum_constrained_test_cases.cpp');
    const sidecarPath = constraintsPathFor(sourcePath);
    const loaded = await loadConstraints(sidecarPath, new SchemaValidator(schemaDirectory));
    assert.equal(loaded.kind, 'ok', 'failed to load sidecar constraints');

    const response = await client!.send(
      analyzeRequest({
        displayPath: '09_sum_constrained_test_cases.cpp',
        content: readFileSync(sourcePath, 'utf8'),
        cppStandard: 'c++20',
        constraints: loaded.constraints,
      }),
    );
    if (response.status === 'error' && response.errors[0]?.code === 'frontend_unavailable') {
      t.skip('engine was built without the Clang frontend');
      return;
    }
    assert.equal(response.status, 'ok', JSON.stringify(response.errors));
    assert.ok(isAnalyzeResults(response.results));
    assert.equal(response.results.summary.max_time_workload, 4e10);

    const finding = response.results.findings.find((f) => f.pattern === 'test-case-loop');
    assert.ok(finding, 'test-case-loop finding not found');
    assert.ok(finding.time, 'finding has no time bound');
    assert.equal(finding.time.workload.upper_bound, 4e10);
    assert.equal(finding.time.workload.method, 'sum-constraint-packing');

    // Assert that the formatted workload detail matches tree item representation
    const workloadDetail = `≤ ${formatCount(finding.time.workload.upper_bound)} [${statusLabel(
      finding.time.workload.status,
    )}, ${finding.time.workload.method}]`;
    assert.equal(workloadDetail, '≤ 4.000e10 [Proven, sum-constraint-packing]');
    assert.match(summarizeFinding(finding), /≤ 4\.000e10 body executions/);
  });
});
