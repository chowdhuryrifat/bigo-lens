import * as path from 'node:path';
import * as vscode from 'vscode';

import { ConstraintFormPanel } from '../panels/constraintFormPanel';
import { constraintsToForm, type DetectedVariable } from '../services/constraintsFormModel';
import { CONSTRAINTS_TEMPLATE, constraintsPathFor, loadConstraints } from '../services/constraintsStore';
import type { ExtensionServices } from '../services/context';
import type { ReportSnapshot } from '../services/reportHtml';

export interface ConfigureConstraintsContext extends ExtensionServices {
  extensionUri: vscode.Uri;
  snapshotProvider: () => ReportSnapshot | undefined;
  onAnalyze?: () => Promise<void> | void;
}

/**
 * Extracts free variables and missing bound names detected during the last analysis.
 */
export function extractDetectedVariables(
  snapshot: ReportSnapshot | undefined,
  currentFileFsPath: string,
): DetectedVariable[] {
  if (!snapshot) {
    return [];
  }

  // Check that the snapshot corresponds to the active file
  const baseCurrent = path.basename(currentFileFsPath);
  const baseSnapshot = path.basename(snapshot.displayPath);
  if (baseCurrent !== baseSnapshot && !currentFileFsPath.endsWith(snapshot.displayPath)) {
    return [];
  }

  const detectedMap = new Map<string, DetectedVariable>();

  for (const finding of snapshot.results.findings) {
    const locHint = `${finding.function} (line ${finding.location.line})`;
    const missing = new Set(finding.time?.workload.missing_bounds ?? []);

    if (finding.time?.variables) {
      for (const v of finding.time.variables) {
        if (!detectedMap.has(v) || missing.has(v)) {
          detectedMap.set(v, {
            name: v,
            status: missing.has(v) ? 'missing_bound' : 'used',
            locationHint: locHint,
          });
        }
      }
    }

    if (finding.space?.variables) {
      for (const v of finding.space.variables) {
        if (!detectedMap.has(v)) {
          detectedMap.set(v, {
            name: v,
            status: 'used',
            locationHint: locHint,
          });
        }
      }
    }
  }

  return Array.from(detectedMap.values());
}

/**
 * Opens the constraint configuration form webview for the active document.
 */
export async function configureConstraints(context: ConfigureConstraintsContext): Promise<void> {
  const editor = vscode.window.activeTextEditor;
  const isSupportedLanguage =
    editor?.document.languageId === 'cpp' ||
    editor?.document.languageId === 'python' ||
    editor?.document.languageId === 'java' ||
    editor?.document.languageId === 'rust' ||
    editor?.document.languageId === 'javascript';

  if (!editor || !isSupportedLanguage || editor.document.uri.scheme !== 'file') {
    void vscode.window.showWarningMessage(
      'BigO Lens: open a supported source file saved on disk to configure its constraints.',
    );
    return;
  }

  const sourceFsPath = editor.document.uri.fsPath;
  const sidecarPath = constraintsPathFor(sourceFsPath);

  const loadOutcome = await loadConstraints(sidecarPath, context.validator);
  let constraints = CONSTRAINTS_TEMPLATE;

  if (loadOutcome.kind === 'ok') {
    constraints = loadOutcome.constraints;
  } else if (loadOutcome.kind === 'invalid') {
    void vscode.window.showWarningMessage(
      `BigO Lens: sidecar file ${path.basename(sidecarPath)} contains errors. Opening form with fallback values.`,
    );
  }

  const detected = extractDetectedVariables(context.snapshotProvider(), sourceFsPath);
  const formState = constraintsToForm(constraints, detected);

  ConstraintFormPanel.createOrShow({
    extensionUri: context.extensionUri,
    displayPath: vscode.workspace.asRelativePath(editor.document.uri),
    sidecarPath,
    initialState: formState,
    validator: context.validator,
    onAnalyze: context.onAnalyze,
  });
}
