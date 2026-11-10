import { join } from 'node:path';

/** <extension>/ when running from out/test/... */
export const extensionRoot = join(__dirname, '..', '..');
export const repoRoot = join(extensionRoot, '..');
export const fakeEnginePath = join(extensionRoot, 'test', 'fixtures', 'fake-engine.mjs');
export const schemaDirectory = join(extensionRoot, 'schemas');
export const examplesDirectory = join(repoRoot, 'shared', 'examples');
export const fixturesDirectory = join(repoRoot, 'tests', 'fixtures', 'cpp');
