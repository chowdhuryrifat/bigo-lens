import * as vscode from 'vscode';

import type { AnalyzeResults, Finding } from '../models/protocol';
import { formatBytes, formatCount, statusLabel, summarizeFinding } from '../services/format';

type Node =
  | { kind: 'section'; label: string; children: Node[]; icon: string }
  | { kind: 'finding'; finding: Finding; uri: vscode.Uri }
  | { kind: 'detail'; label: string; description?: string; tooltip?: string };

export interface ResultsSnapshot {
  uri: vscode.Uri;
  results: AnalyzeResults;
  engineVersion: string;
}

/** Sidebar tree showing the latest analysis. Holds no state beyond the snapshot. */
export class ResultsTreeProvider implements vscode.TreeDataProvider<Node> {
  private readonly changed = new vscode.EventEmitter<Node | undefined>();
  readonly onDidChangeTreeData = this.changed.event;
  private roots: Node[] = [];

  show(snapshot: ResultsSnapshot | undefined): void {
    this.roots = snapshot ? buildTree(snapshot) : [];
    this.changed.fire(undefined);
  }

  dispose(): void {
    this.changed.dispose();
  }

  getChildren(node?: Node): Node[] {
    if (!node) {
      return this.roots;
    }
    if (node.kind === 'section') {
      return node.children;
    }
    if (node.kind === 'finding') {
      return findingDetails(node.finding);
    }
    return [];
  }

  getTreeItem(node: Node): vscode.TreeItem {
    switch (node.kind) {
      case 'section': {
        const item = new vscode.TreeItem(node.label, vscode.TreeItemCollapsibleState.Expanded);
        item.iconPath = new vscode.ThemeIcon(node.icon);
        item.accessibilityInformation = {
          label: `Section: ${node.label}`,
        };
        return item;
      }
      case 'finding': {
        const f = node.finding;
        const item = new vscode.TreeItem(
          `${f.id} ${f.function ? `${f.function}() ` : ''}line ${f.location.line}`,
          vscode.TreeItemCollapsibleState.Collapsed,
        );
        item.description = summarizeFinding(f);
        item.tooltip = new vscode.MarkdownString(findingTooltip(f));
        item.iconPath = new vscode.ThemeIcon(iconFor(f));
        item.accessibilityInformation = {
          label: `${f.id} ${f.function ? `${f.function} ` : ''}line ${f.location.line}, ${summarizeFinding(f)}, bound status ${f.bound_status}, confidence ${f.confidence}`,
        };
        const line = Math.max(0, f.location.line - 1);
        const column = Math.max(0, f.location.column - 1);
        item.command = {
          command: 'vscode.open',
          title: 'Go to finding',
          arguments: [node.uri, { selection: new vscode.Range(line, column, line, column) }],
        };
        return item;
      }
      case 'detail': {
        const item = new vscode.TreeItem(node.label, vscode.TreeItemCollapsibleState.None);
        if (node.description !== undefined) {
          item.description = node.description;
        }
        if (node.tooltip !== undefined) {
          item.tooltip = node.tooltip;
        }
        item.accessibilityInformation = {
          label: `${node.label}${node.description ? `: ${node.description}` : ''}`,
        };
        return item;
      }
    }
  }
}

function iconFor(finding: Finding): string {
  switch (finding.bound_status) {
    case 'proven':
      return 'pass';
    case 'estimated':
      return 'info';
    case 'unknown':
      return 'question';
    case 'unsupported':
      return 'circle-slash';
  }
}

function findingTooltip(f: Finding): string {
  const lines = [`**${f.pattern}** — ${f.description}`, '', `Bound status: ${statusLabel(f.bound_status)}`];
  lines.push(`Confidence: ${f.confidence}`, `Provenance: ${f.provenance}`);
  return lines.join('\n\n');
}

function findingDetails(f: Finding): Node[] {
  const details: Node[] = [];
  for (const [label, quantity] of [
    ['Time (structural count)', f.time],
    ['Space (bytes)', f.space],
  ] as const) {
    if (!quantity) {
      continue;
    }
    details.push({ kind: 'detail', label, description: quantity.expression, tooltip: quantity.expression });
    details.push({
      kind: 'detail',
      label: '  Asymptotic',
      description: quantity.asymptotic ?? 'not derived',
    });
    const w = quantity.workload;
    details.push({
      kind: 'detail',
      label: '  Workload under constraints',
      description:
        w.upper_bound === null
          ? `unknown${w.missing_bounds.length ? ` (missing bounds: ${w.missing_bounds.join(', ')})` : ''}`
          : `≤ ${formatCount(w.upper_bound)} [${statusLabel(w.status)}${w.method ? `, ${w.method}` : ''}]`,
    });
    for (const assumption of w.assumptions) {
      details.push({ kind: 'detail', label: '    assumption', description: assumption, tooltip: assumption });
    }
  }
  if (!f.time && !f.space) {
    details.push({ kind: 'detail', label: 'No bound reported', description: statusLabel(f.bound_status) });
  }
  for (const assumption of f.assumptions) {
    details.push({ kind: 'detail', label: 'Assumption', description: assumption, tooltip: assumption });
  }
  for (const limitation of f.limitations) {
    details.push({ kind: 'detail', label: 'Limitation', description: limitation, tooltip: limitation });
  }
  return details;
}

function buildTree(snapshot: ResultsSnapshot): Node[] {
  const { results, uri } = snapshot;
  const findings: Node[] = results.findings.map((finding) => ({ kind: 'finding', finding, uri }));
  if (findings.length === 0) {
    findings.push({ kind: 'detail', label: 'No loops, recursion or storage findings detected' });
  }
  const m = results.memory;
  const memory: Node[] = [
    { kind: 'detail', label: 'Static storage', description: formatBytes(m.static_storage_bytes) },
    { kind: 'detail', label: 'Largest local array', description: formatBytes(m.largest_local_array_bytes) },
    {
      kind: 'detail',
      label: 'Estimated minimum',
      description: `${formatBytes(m.estimated_minimum_bytes)} [${statusLabel(m.estimated_minimum_status)}]`,
    },
    ...m.assumptions.map((a): Node => ({ kind: 'detail', label: 'Assumption', description: a, tooltip: a })),
  ];
  const fz = results.feasibility;
  const feasibility: Node[] = [
    { kind: 'detail', label: 'Time', description: fz.time.status, tooltip: fz.time.reason },
    { kind: 'detail', label: 'Memory', description: fz.memory.status, tooltip: fz.memory.reason },
  ];
  return [
    {
      kind: 'section',
      label: `Findings (${results.findings.length})`,
      children: findings,
      icon: 'list-tree',
    },
    { kind: 'section', label: 'Memory (lower bounds)', children: memory, icon: 'database' },
    { kind: 'section', label: 'Feasibility', children: feasibility, icon: 'dashboard' },
    {
      kind: 'section',
      label: 'Engine',
      icon: 'chip',
      children: [
        { kind: 'detail', label: 'Version', description: snapshot.engineVersion },
        { kind: 'detail', label: 'Coverage', description: results.summary.coverage },
      ],
    },
  ];
}
