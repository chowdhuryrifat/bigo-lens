import * as vscode from 'vscode';

import { ReportPanel } from '../panels/reportPanel';
import type { ReportSnapshot } from '../services/reportHtml';

export function openReport(
  extensionUri: vscode.Uri,
  snapshotProvider: () => ReportSnapshot | undefined,
): void {
  ReportPanel.createOrShow(extensionUri, snapshotProvider());
}
