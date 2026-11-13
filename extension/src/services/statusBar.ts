import * as vscode from 'vscode';

import type { AnalyzeResults, FeasibilityStatus, Finding } from '../models/protocol';
import { formatCount } from './format';

export const OPEN_REPORT_COMMAND = 'bigoLens.openReport';

export function formatFeasibilityIndicator(status: FeasibilityStatus): string {
  switch (status) {
    case 'likely_safe':
      return '✓';
    case 'borderline':
      return '⚠';
    case 'high_risk':
      return '✕';
    case 'unknown':
      return '?';
  }
}

export function getDominantFinding(results: AnalyzeResults): Finding | undefined {
  if (results.summary.dominant_finding_id) {
    const found = results.findings.find((f) => f.id === results.summary.dominant_finding_id);
    if (found) {
      return found;
    }
  }
  return results.findings[0];
}

export function formatStatusBarText(results: AnalyzeResults): string {
  const dominant = getDominantFinding(results);
  const asymptotic =
    dominant?.time?.asymptotic ??
    dominant?.space?.asymptotic ??
    dominant?.time?.expression ??
    (results.findings.length === 0 ? 'none' : 'O(?)');

  const timeIndicator = formatFeasibilityIndicator(results.feasibility.time.status);
  const memIndicator = formatFeasibilityIndicator(results.feasibility.memory.status);

  if (results.summary.max_time_workload !== null) {
    const ops = formatCount(results.summary.max_time_workload);
    return `BigO: ${asymptotic} (≤ ${ops}) · time ${timeIndicator} · mem ${memIndicator}`;
  }

  return `BigO: ${asymptotic} · time ${timeIndicator} · mem ${memIndicator}`;
}

export function formatStatusBarTooltip(results: AnalyzeResults, displayPath?: string): vscode.MarkdownString {
  const dominant = getDominantFinding(results);
  const asymptotic =
    dominant?.time?.asymptotic ?? dominant?.space?.asymptotic ?? dominant?.time?.expression ?? 'O(?)';
  const workloadStr =
    dominant?.time?.workload.upper_bound !== null && dominant?.time?.workload.upper_bound !== undefined
      ? `≤ ${formatCount(dominant.time.workload.upper_bound)} ops`
      : 'unknown';

  const md = new vscode.MarkdownString('', true);
  md.isTrusted = true;
  md.appendMarkdown(`### BigO Lens Analysis\n\n`);
  if (displayPath) {
    md.appendMarkdown(`**File:** \`${displayPath}\`\n\n`);
  }
  md.appendMarkdown(`**Dominant Complexity:** ${asymptotic} (${dominant?.pattern ?? 'finding'})\n\n`);
  md.appendMarkdown(
    `**Workload:** ${workloadStr} [${dominant?.bound_status ?? 'unknown'}, confidence: ${dominant?.confidence ?? 'none'}]\n\n`,
  );
  md.appendMarkdown(`---\n\n`);
  md.appendMarkdown(`**Feasibility:**\n`);
  md.appendMarkdown(`- Time: \`${results.feasibility.time.status}\` (${results.feasibility.time.reason})\n`);
  md.appendMarkdown(
    `- Memory: \`${results.feasibility.memory.status}\` (${results.feasibility.memory.reason})\n\n`,
  );
  md.appendMarkdown(`[Open Detailed Report](command:${OPEN_REPORT_COMMAND})`);
  return md;
}

export function isCppDocument(document: vscode.TextDocument): boolean {
  return document.languageId === 'cpp' || document.languageId === 'c';
}

export interface FileAnalysisRecord {
  results: AnalyzeResults;
  displayPath: string;
}

export class StatusBarManager implements vscode.Disposable {
  private readonly item: vscode.StatusBarItem;
  private readonly fileResults = new Map<string, FileAnalysisRecord>();

  constructor(statusBarItem?: vscode.StatusBarItem) {
    this.item = statusBarItem ?? vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Right, 100);
    this.item.command = OPEN_REPORT_COMMAND;
    this.item.name = 'BigO Lens Complexity';
  }

  public get statusBarItem(): vscode.StatusBarItem {
    return this.item;
  }

  public updateResults(uri: vscode.Uri, results: AnalyzeResults, displayPath: string): void {
    this.fileResults.set(uri.toString(), { results, displayPath });
    const activeEditor = vscode.window.activeTextEditor;
    if (activeEditor && activeEditor.document.uri.toString() === uri.toString()) {
      this.refresh(activeEditor);
    }
  }

  public onActiveEditorChange(editor: vscode.TextEditor | undefined): void {
    this.refresh(editor);
  }

  public refresh(editor: vscode.TextEditor | undefined): void {
    if (!editor || !isCppDocument(editor.document)) {
      this.item.hide();
      return;
    }

    const record = this.fileResults.get(editor.document.uri.toString());
    if (!record) {
      this.item.hide();
      return;
    }

    this.item.text = `$(graph) ${formatStatusBarText(record.results)}`;
    this.item.tooltip = formatStatusBarTooltip(record.results, record.displayPath);
    this.item.show();
  }

  public clear(): void {
    this.fileResults.clear();
    this.item.hide();
  }

  public dispose(): void {
    this.item.dispose();
  }
}
