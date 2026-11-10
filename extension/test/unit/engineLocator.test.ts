import assert from 'node:assert/strict';
import { join } from 'node:path';
import { describe, it } from 'node:test';

import { locateEngine } from '../../src/services/engineLocator';

const root = join('/', 'opt', 'ext', 'extension');
const base = { environmentPath: undefined, extensionRoot: root, platform: 'linux' as NodeJS.Platform };

describe('locateEngine', () => {
  it('prefers the configured absolute path', () => {
    const configured = join('/', 'tools', 'bigolens-engine');
    const result = locateEngine({ ...base, configuredPath: configured, isExecutable: () => true });
    assert.equal(result.path, configured);
  });

  it('rejects a relative configured path instead of guessing', () => {
    const result = locateEngine({ ...base, configuredPath: 'bin/engine', isExecutable: () => true });
    assert.equal(result.path, undefined);
    assert.match(result.problem ?? '', /absolute path/);
  });

  it('does not fall back when the configured path is unusable', () => {
    const result = locateEngine({ ...base, configuredPath: join('/', 'missing'), isExecutable: () => false });
    assert.equal(result.path, undefined);
    assert.match(result.problem ?? '', /does not point to an executable/);
  });

  it('uses the environment variable when no setting is given', () => {
    const fromEnv = join('/', 'env', 'bigolens-engine');
    const result = locateEngine({
      ...base,
      configuredPath: '',
      environmentPath: fromEnv,
      isExecutable: (p) => p === fromEnv,
    });
    assert.equal(result.path, fromEnv);
  });

  it('finds the bundled engine before development builds', () => {
    const bundled = join(root, 'bin', 'bigolens-engine');
    const result = locateEngine({ ...base, configuredPath: '', isExecutable: () => true });
    assert.equal(result.path, bundled);
  });

  it('finds a development build and uses .exe on Windows', () => {
    const dev = join(root, '..', 'build', 'release', 'engine', 'Release', 'bigolens-engine.exe');
    const result = locateEngine({
      ...base,
      platform: 'win32',
      configuredPath: '',
      isExecutable: (p) => p === dev,
    });
    assert.equal(result.path, dev);
  });

  it('reports every searched location when nothing is found', () => {
    const result = locateEngine({ ...base, configuredPath: '', isExecutable: () => false });
    assert.equal(result.path, undefined);
    assert.ok(result.searched.length > 4);
    assert.equal(result.problem, undefined);
  });
});
