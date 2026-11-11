// Smoke tests executed inside the VS Code extension host.
import assert from 'node:assert/strict';
import { join } from 'node:path';
import * as vscode from 'vscode';

const EXTENSION_ID = 'bigo-lens.bigo-lens';

async function check(name: string, body: () => Promise<void>): Promise<void> {
  try {
    await body();
    process.stdout.write(`  ok   ${name}\n`);
  } catch (err) {
    process.stdout.write(`  FAIL ${name}\n`);
    throw err;
  }
}

export async function run(): Promise<void> {
  await check('extension is present', () => {
    assert.ok(vscode.extensions.getExtension(EXTENSION_ID), `${EXTENSION_ID} not installed`);
    return Promise.resolve();
  });

  await check('extension activates when a C++ file is opened', async () => {
    const folder = vscode.workspace.workspaceFolders?.[0];
    assert.ok(folder, 'fixture workspace not opened');
    const document = await vscode.workspace.openTextDocument(join(folder.uri.fsPath, '01_linear_loop.cpp'));
    await vscode.window.showTextDocument(document);
    const extension = vscode.extensions.getExtension(EXTENSION_ID);
    await extension?.activate();
    assert.equal(extension?.isActive, true);
  });

  await check('commands are registered', async () => {
    const commands = await vscode.commands.getCommands(true);
    for (const id of [
      'bigoLens.analyzeActiveFile',
      'bigoLens.editConstraints',
      'bigoLens.showCapabilities',
    ]) {
      assert.ok(commands.includes(id), `${id} missing`);
    }
  });

  if (process.env.BIGOLENS_E2E_REQUIRE_ENGINE === '1') {
    await check('analyzing a fixture publishes findings from the engine', async () => {
      const folder = vscode.workspace.workspaceFolders?.[0];
      assert.ok(folder);
      const uri = vscode.Uri.file(join(folder.uri.fsPath, '01_linear_loop.cpp'));
      await vscode.window.showTextDocument(await vscode.workspace.openTextDocument(uri));
      await vscode.commands.executeCommand('bigoLens.analyzeActiveFile');
      const deadline = Date.now() + 60000;
      let findings: vscode.Diagnostic[] = [];
      while (Date.now() < deadline) {
        findings = vscode.languages.getDiagnostics(uri).filter((d) => d.source === 'BigO Lens');
        if (findings.length > 0) {
          break;
        }
        await new Promise((resolve) => setTimeout(resolve, 250));
      }
      assert.ok(findings.length > 0, 'no BigO Lens diagnostics were published');
      assert.ok(
        findings.some((d) => d.message.includes('linear-loop')),
        findings.map((d) => d.message).join('\n'),
      );
    });

    await check('analyzing sum-constrained fixture with sidecar shows 4e10 workload', async () => {
      const folder = vscode.workspace.workspaceFolders?.[0];
      assert.ok(folder);
      const uri = vscode.Uri.file(join(folder.uri.fsPath, '09_sum_constrained_test_cases.cpp'));
      await vscode.window.showTextDocument(await vscode.workspace.openTextDocument(uri));
      await vscode.commands.executeCommand('bigoLens.analyzeActiveFile');
      const deadline = Date.now() + 60000;
      let findings: vscode.Diagnostic[] = [];
      while (Date.now() < deadline) {
        findings = vscode.languages.getDiagnostics(uri).filter((d) => d.source === 'BigO Lens');
        if (findings.length > 0) {
          break;
        }
        await new Promise((resolve) => setTimeout(resolve, 250));
      }
      assert.ok(findings.length > 0, 'no BigO Lens diagnostics were published for sum-constrained fixture');
      assert.ok(
        findings.some((d) => d.message.includes('4.000e10') || d.message.includes('test-case-loop')),
        findings.map((d) => d.message).join('\n'),
      );
    });
  }
}
