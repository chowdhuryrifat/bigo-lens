import * as vscode from 'vscode';

import type { AnalyzeResults, Diagnostic as EngineDiagnostic } from '../models/protocol';
import { summarizeFinding } from '../services/format';

/** Publishes findings and compiler diagnostics to the Problems panel. */
export class DiagnosticsPublisher implements vscode.Disposable {
  private readonly collection = vscode.languages.createDiagnosticCollection('bigo-lens');

  publish(uri: vscode.Uri, results: AnalyzeResults | null, engineDiagnostics: EngineDiagnostic[]): void {
    const diagnostics: vscode.Diagnostic[] = [];
    for (const d of engineDiagnostics) {
      if (!d.location) {
        continue;
      }
      const range = toRange(d.location.line, d.location.column);
      const severity =
        d.severity === 'error'
          ? vscode.DiagnosticSeverity.Error
          : d.severity === 'warning'
            ? vscode.DiagnosticSeverity.Warning
            : vscode.DiagnosticSeverity.Information;
      const diagnostic = new vscode.Diagnostic(range, d.message, severity);
      diagnostic.source = 'BigO Lens (clang)';
      diagnostics.push(diagnostic);
    }
    for (const finding of results?.findings ?? []) {
      const diagnostic = new vscode.Diagnostic(
        toRange(finding.location.line, finding.location.column),
        `${finding.id}: ${summarizeFinding(finding)}`,
        vscode.DiagnosticSeverity.Information,
      );
      diagnostic.source = 'BigO Lens';
      diagnostic.code = finding.pattern;
      diagnostics.push(diagnostic);
    }
    this.collection.set(uri, diagnostics);
  }

  clear(uri: vscode.Uri): void {
    this.collection.delete(uri);
  }

  dispose(): void {
    this.collection.dispose();
  }
}

function toRange(line: number, column: number): vscode.Range {
  const l = Math.max(0, line - 1);
  const c = Math.max(0, column - 1);
  return new vscode.Range(l, c, l, c + 1);
}
