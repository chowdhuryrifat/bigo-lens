import { readFile, writeFile } from 'node:fs/promises';

import type { Constraints } from '../models/protocol';
import { findJsonPointerRange, type TextRange } from './jsonPointer';
import type { SchemaValidator } from './schemaValidator';

/** Sidecar file holding the constraints for `<file>`: `<file>.bigolens.json`. */
export function constraintsPathFor(sourcePath: string): string {
  return `${sourcePath}.bigolens.json`;
}

/** Upper bound on the sidecar file size; constraints are small documents. */
export const MAX_CONSTRAINTS_FILE_BYTES = 256 * 1024;

export const CONSTRAINTS_TEMPLATE: Constraints = {
  variables: {
    t: { min: 1, max: 10 },
    n: { min: 1, max: 200000 },
  },
  sum_constraints: [{ variable: 'n', count_variable: 't', max_total: 200000 }],
  time_limit_ms: 2000,
  memory_limit_mb: 256,
};

export interface ConstraintDiagnostic {
  message: string;
  path?: string | undefined;
  range?: TextRange | undefined;
}

export type ConstraintsLoad =
  | { kind: 'missing' }
  | { kind: 'ok'; constraints: Constraints }
  | { kind: 'invalid'; errors: string[]; diagnostics: ConstraintDiagnostic[] };

export async function loadConstraints(path: string, validator: SchemaValidator): Promise<ConstraintsLoad> {
  let text: string;
  try {
    text = await readFile(path, 'utf8');
  } catch (err) {
    if ((err as NodeJS.ErrnoException).code === 'ENOENT') {
      return { kind: 'missing' };
    }
    const message = `cannot read ${path}: ${(err as Error).message}`;
    return { kind: 'invalid', errors: [message], diagnostics: [{ message }] };
  }
  if (Buffer.byteLength(text, 'utf8') > MAX_CONSTRAINTS_FILE_BYTES) {
    const message = `${path} is larger than ${MAX_CONSTRAINTS_FILE_BYTES} bytes`;
    return { kind: 'invalid', errors: [message], diagnostics: [{ message }] };
  }
  let document: unknown;
  try {
    document = JSON.parse(text);
  } catch (err) {
    const message = `${path} is not valid JSON: ${(err as Error).message}`;
    return { kind: 'invalid', errors: [message], diagnostics: [{ message }] };
  }
  const outcome = validator.validateConstraints(document);
  if (!outcome.value) {
    const diagnostics: ConstraintDiagnostic[] = (outcome.details ?? []).map((d) => ({
      message: d.message,
      path: d.path,
      range: findJsonPointerRange(text, d.path),
    }));
    return { kind: 'invalid', errors: outcome.errors, diagnostics };
  }

  // Check for inverted variable bounds: min > max
  const diagnostics: ConstraintDiagnostic[] = [];
  const errors: string[] = [];
  if (outcome.value.variables) {
    for (const [name, bound] of Object.entries(outcome.value.variables)) {
      if (bound.min !== undefined && bound.max !== undefined && bound.min > bound.max) {
        const message = `variable '${name}' has min (${bound.min}) greater than max (${bound.max})`;
        errors.push(message);
        diagnostics.push({
          message,
          path: `/variables/${name}`,
          range: findJsonPointerRange(text, `/variables/${name}`),
        });
      }
    }
  }

  if (errors.length > 0) {
    return { kind: 'invalid', errors, diagnostics };
  }

  return { kind: 'ok', constraints: outcome.value };
}

/** Writes the template only if no file exists yet (never overwrites). */
export async function createConstraintsTemplate(path: string): Promise<boolean> {
  try {
    await writeFile(path, `${JSON.stringify(CONSTRAINTS_TEMPLATE, null, 2)}\n`, {
      encoding: 'utf8',
      flag: 'wx',
    });
    return true;
  } catch (err) {
    if ((err as NodeJS.ErrnoException).code === 'EEXIST') {
      return false;
    }
    throw err;
  }
}
