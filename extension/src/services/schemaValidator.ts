import Ajv2020, { type ErrorObject, type ValidateFunction } from 'ajv/dist/2020';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';

import type { Constraints, EngineRequest, EngineResponse } from '../models/protocol';

/** Directory holding the schemas copied from shared/schemas at build time. */
export function defaultSchemaDirectory(): string {
  // Compiled location: <extension>/out/src/services
  return join(__dirname, '..', '..', '..', 'schemas');
}

export interface ValidationError {
  message: string;
  path: string;
}

export interface ValidationOutcome<T> {
  value?: T;
  errors: string[];
  details?: ValidationError[];
}

function describe(errors: ErrorObject[] | null | undefined): string[] {
  return (errors ?? []).map((e) => `${e.instancePath || '/'} ${e.message ?? 'is invalid'}`);
}

function errorDetails(errors: ErrorObject[] | null | undefined): ValidationError[] {
  return (errors ?? []).map((e) => ({
    message: `${e.instancePath || '/'} ${e.message ?? 'is invalid'}`,
    path: e.instancePath || '/',
  }));
}

/** Validates protocol documents at the extension boundary. */
export class SchemaValidator {
  private readonly request: ValidateFunction;
  private readonly response: ValidateFunction;
  private readonly constraints: ValidateFunction;

  constructor(schemaDirectory: string = defaultSchemaDirectory()) {
    const ajv = new Ajv2020({ allErrors: true, strict: true, allowUnionTypes: true });
    const load = (name: string): Record<string, unknown> =>
      JSON.parse(readFileSync(join(schemaDirectory, name), 'utf8')) as Record<string, unknown>;
    const requestSchema = load('request.schema.json');
    ajv.addSchema(requestSchema);
    this.request = ajv.compile(requestSchema);
    this.response = ajv.compile(load('response.schema.json'));
    const requestId = requestSchema.$id;
    if (typeof requestId !== 'string') {
      throw new Error('request schema has no $id');
    }
    this.constraints = ajv.compile({ $ref: `${requestId}#/$defs/constraints` });
  }

  validateRequest(document: unknown): ValidationOutcome<EngineRequest> {
    return this.run<EngineRequest>(this.request, document);
  }

  validateResponse(document: unknown): ValidationOutcome<EngineResponse> {
    return this.run<EngineResponse>(this.response, document);
  }

  validateConstraints(document: unknown): ValidationOutcome<Constraints> {
    return this.run<Constraints>(this.constraints, document);
  }

  private run<T>(validate: ValidateFunction, document: unknown): ValidationOutcome<T> {
    if (validate(document)) {
      return { value: document as T, errors: [], details: [] };
    }
    return { errors: describe(validate.errors), details: errorDetails(validate.errors) };
  }
}
