import * as vscode from 'vscode';

import { isCapabilitiesResults } from '../models/protocol';
import { createEngineClient, type ExtensionServices } from '../services/context';
import { capabilitiesRequest } from '../services/requestBuilder';

export async function showCapabilities(services: ExtensionServices): Promise<void> {
  const client = createEngineClient(services);
  if (typeof client === 'string') {
    void vscode.window.showErrorMessage(`BigO Lens: ${client}`);
    return;
  }
  try {
    const response = await client.send(capabilitiesRequest());
    const out = services.output;
    out.appendLine(
      `Engine ${response.engine.name} ${response.engine.version} (protocol ${response.engine.protocol_version})`,
    );
    out.appendLine(`Frontend: ${response.engine.frontend ?? 'none (built without Clang)'}`);
    if (isCapabilitiesResults(response.results)) {
      out.appendLine('Features:');
      for (const [feature, status] of Object.entries(response.results.features)) {
        out.appendLine(`  ${feature}: ${status}`);
      }
    }
    out.show(true);
  } catch (err) {
    void vscode.window.showErrorMessage(`BigO Lens: ${err instanceof Error ? err.message : String(err)}`);
  }
}
