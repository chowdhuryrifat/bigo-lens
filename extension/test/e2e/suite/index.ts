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
}
