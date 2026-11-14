import { randomUUID } from 'node:crypto';

import {
  PROTOCOL_VERSION,
  type BenchmarkConfig,
  type Constraints,
  type CppStandard,
  type EngineRequest,
} from '../models/protocol';

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
  cppStandard?: CppStandard | undefined;
  language?: string | undefined;
}

export function analyzeRequest(input: AnalyzeInput, requestId: string = newRequestId()): EngineRequest {
  const request: EngineRequest = {
    schema_version: PROTOCOL_VERSION,
    request_id: requestId,
    action: 'analyze',
    source: { path: input.displayPath, language: input.language ?? 'cpp', content: input.content },
  };
  if (input.cppStandard) {
    request.options = { cpp_standard: input.cppStandard };
  }
  if (input.constraints) {
    request.constraints = input.constraints;
  }
  return request;
}

export interface BenchmarkInput extends AnalyzeInput {
  benchmarkConfig?: BenchmarkConfig | undefined;
}

export function benchmarkRequest(input: BenchmarkInput, requestId: string = newRequestId()): EngineRequest {
  const request = analyzeRequest(input, requestId);
  request.action = 'benchmark';
  if (input.benchmarkConfig) {
    request.benchmark_config = input.benchmarkConfig;
  }
  return request;
}
