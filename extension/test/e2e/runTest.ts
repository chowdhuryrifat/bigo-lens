// Launches a real VS Code instance with the extension under development and
// runs the smoke suite inside the extension host.
import { runTests } from '@vscode/test-electron';
import { join } from 'node:path';

async function main(): Promise<void> {
  const extensionDevelopmentPath = join(__dirname, '..', '..', '..');
  const extensionTestsPath = join(__dirname, 'suite', 'index');
  const workspace = join(extensionDevelopmentPath, '..', 'tests', 'fixtures', 'cpp');
  // When launched from a VS Code terminal this variable is inherited and would
  // make the downloaded VS Code behave as plain Node.js.
  delete process.env.ELECTRON_RUN_AS_NODE;
  await runTests({
    extensionDevelopmentPath,
    extensionTestsPath,
    launchArgs: [workspace, '--disable-extensions', '--disable-workspace-trust'],
  });
}

main().catch((err: unknown) => {
  process.stderr.write(`VS Code tests failed: ${String(err)}\n`);
  process.exit(1);
});
