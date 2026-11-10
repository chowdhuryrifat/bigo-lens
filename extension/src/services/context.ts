import * as vscode from 'vscode';

import type { CppStandard } from '../models/protocol';
import { EngineClient } from './engineClient';
import { locateEngine } from './engineLocator';
import type { SchemaValidator } from './schemaValidator';

/** Shared, explicitly passed dependencies of the commands (no global state). */
export interface ExtensionServices {
  extensionRoot: string;
  validator: SchemaValidator;
  output: vscode.OutputChannel;
}

export function readSettings(): { enginePath: string; cppStandard: CppStandard; timeoutMs: number } {
  const config = vscode.workspace.getConfiguration('bigoLens');
  return {
    enginePath: config.get<string>('enginePath', ''),
    cppStandard: config.get<CppStandard>('cppStandard', 'c++20'),
    timeoutMs: config.get<number>('analysisTimeoutMs', 30000),
  };
}

/** Creates a client for the configured engine, or explains why none was found. */
export function createEngineClient(services: ExtensionServices): EngineClient | string {
  const settings = readSettings();
  const located = locateEngine({
    configuredPath: settings.enginePath,
    environmentPath: process.env.BIGOLENS_ENGINE_PATH,
    extensionRoot: services.extensionRoot,
    platform: process.platform,
  });
  if (!located.path) {
    services.output.appendLine('Engine search order:');
    for (const candidate of located.searched) {
      services.output.appendLine(`  ${candidate}`);
    }
    return (
      located.problem ??
      'The BigO Lens engine was not found. Build it (cmake --preset dev && cmake --build --preset dev) or set bigoLens.enginePath.'
    );
  }
  services.output.appendLine(`Using engine: ${located.path}`);
  return new EngineClient({
    enginePath: located.path,
    timeoutMs: settings.timeoutMs,
    validator: services.validator,
  });
}
