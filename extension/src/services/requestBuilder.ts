import { randomUUID } from 'node:crypto';

import { PROTOCOL_VERSION, type Constraints, type CppStandard, type EngineRequest } from '../models/protocol';

export function newRequestId(): string {
  return `vscode-${randomUUID()}`;
}

export function capabilitiesRequest(requestId: string = newRequestId()): EngineRequest {
  return { schema_version: PROTOCOL_VERSION, request_id: requestId, action: 'capabilities' };
}

export interface AnalyzeInput {
  displayPath: string;
  content: string;
  constraints: Constraints | undefined;
  cppStandard: CppStandard;
}

export function analyzeRequest(input: AnalyzeInput, requestId: string = newRequestId()): EngineRequest {
  const request: EngineRequest = {
    schema_version: PROTOCOL_VERSION,
    request_id: requestId,
    action: 'analyze',
    source: { path: input.displayPath, language: 'cpp', content: input.content },
    options: { cpp_standard: input.cppStandard },
  };
  if (input.constraints) {
    request.constraints = input.constraints;
  }
  return request;
}
