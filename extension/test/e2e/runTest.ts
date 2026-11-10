// Launches a real VS Code instance with the extension under development and
// runs the smoke suite inside the extension host.
import { runTests } from '@vscode/test-electron';
import { join } from 'node:path';

async function main(): Promise<void> {
  // BIGOLENS_E2E_EXTENSION_PATH runs the suite against another copy of the
  // extension, e.g. an unpacked .vsix, so its bundled engine is exercised.
  const extensionDevelopmentPath =
    process.env.BIGOLENS_E2E_EXTENSION_PATH ?? join(__dirname, '..', '..', '..');
  const extensionTestsPath = join(__dirname, 'suite', 'index');
  const workspace = join(__dirname, '..', '..', '..', '..', 'tests', 'fixtures', 'cpp');
  // When launched from a VS Code terminal this variable is inherited and would
  // make the downloaded VS Code behave as plain Node.js.
  delete process.env.ELECTRON_RUN_AS_NODE;
  await runTests({
    extensionDevelopmentPath,
    extensionTestsPath,
    launchArgs: [workspace, '--disable-extensions', '--disable-workspace-trust'],
    extensionTestsEnv: { BIGOLENS_E2E_REQUIRE_ENGINE: process.env.BIGOLENS_E2E_REQUIRE_ENGINE ?? '' },
  });
}

main().catch((err: unknown) => {
  process.stderr.write(`VS Code tests failed: ${String(err)}\n`);
  process.exit(1);
});
