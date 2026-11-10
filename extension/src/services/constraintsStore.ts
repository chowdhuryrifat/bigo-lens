import { readFile, writeFile } from 'node:fs/promises';

import type { Constraints } from '../models/protocol';
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

export type ConstraintsLoad =
  { kind: 'missing' } | { kind: 'ok'; constraints: Constraints } | { kind: 'invalid'; errors: string[] };

export async function loadConstraints(path: string, validator: SchemaValidator): Promise<ConstraintsLoad> {
  let text: string;
  try {
    text = await readFile(path, 'utf8');
  } catch (err) {
    if ((err as NodeJS.ErrnoException).code === 'ENOENT') {
      return { kind: 'missing' };
    }
    return { kind: 'invalid', errors: [`cannot read ${path}: ${(err as Error).message}`] };
  }
  if (Buffer.byteLength(text, 'utf8') > MAX_CONSTRAINTS_FILE_BYTES) {
    return { kind: 'invalid', errors: [`${path} is larger than ${MAX_CONSTRAINTS_FILE_BYTES} bytes`] };
  }
  let document: unknown;
  try {
    document = JSON.parse(text);
  } catch (err) {
    return { kind: 'invalid', errors: [`${path} is not valid JSON: ${(err as Error).message}`] };
  }
  const outcome = validator.validateConstraints(document);
  return outcome.value
    ? { kind: 'ok', constraints: outcome.value }
    : { kind: 'invalid', errors: outcome.errors };
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
