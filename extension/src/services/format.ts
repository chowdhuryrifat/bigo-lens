import type { BoundStatus, Finding } from '../models/protocol';

/** Formats large counts compactly without implying more precision than exists. */
export function formatCount(value: number | null): string {
  if (value === null) {
    return 'unknown';
  }
  if (!Number.isFinite(value)) {
    return 'overflow';
  }
  if (Math.abs(value) < 1e6) {
    return Number.isInteger(value) ? value.toString() : value.toPrecision(4);
  }
  return value.toExponential(3).replace('e+', 'e');
}

export function formatBytes(bytes: number | null): string {
  if (bytes === null) {
    return 'unknown';
  }
  const units = ['B', 'KiB', 'MiB', 'GiB', 'TiB'];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit++;
  }
  return `${unit === 0 ? value.toString() : value.toFixed(2)} ${units[unit] ?? 'B'}`;
}

export function statusLabel(status: BoundStatus): string {
  return status.charAt(0).toUpperCase() + status.slice(1);
}

/** One-line summary shown in the tree and in diagnostics. */
export function summarizeFinding(finding: Finding): string {
  const quantity = finding.time ?? finding.space;
  const parts = [finding.pattern];
  if (quantity) {
    parts.push(quantity.asymptotic ?? quantity.expression);
    if (quantity.workload.upper_bound !== null) {
      parts.push(
        `≤ ${formatCount(quantity.workload.upper_bound)}${finding.time ? ' body executions' : ' bytes'}`,
      );
    }
  }
  parts.push(`[${statusLabel(finding.bound_status)}, confidence ${finding.confidence}]`);
  return parts.join(' · ');
}
