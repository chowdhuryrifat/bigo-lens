/**
 * TypeScript view of protocol version 1.0 (shared/schemas/*.schema.json).
 * Responses are validated against the JSON schema before these types are trusted.
 */

export const PROTOCOL_VERSION = '1.0';

export type Action = 'capabilities' | 'analyze' | 'benchmark';
export type CppStandard = 'c++11' | 'c++14' | 'c++17' | 'c++20' | 'c++23';
export type BoundStatus = 'proven' | 'estimated' | 'unknown' | 'unsupported';
export type Confidence = 'high' | 'medium' | 'low' | 'none';
export type FeasibilityStatus = 'likely_safe' | 'borderline' | 'high_risk' | 'unknown';

export interface VariableBound {
  min?: number;
  max?: number;
}

export interface Constraints {
  variables?: Record<string, VariableBound>;
  sum_constraints?: { variable: string; count_variable: string; max_total: number }[];
  product_constraints?: { variables: string[]; max_product: number }[];
  linear_constraints?: { coefficients: Record<string, number>; bound: number }[];
  time_limit_ms?: number;
  memory_limit_mb?: number;
}

export interface BenchmarkConfig {
  sizes?: number[];
  repetitions?: number;
  warmup?: number;
  seed?: number;
  per_run_timeout_ms?: number;
  allow_unsandboxed?: boolean;
}

export interface EngineRequest {
  schema_version: string;
  request_id: string;
  action: Action;
  source?: { path: string; language: string; content: string };
  constraints?: Constraints;
  options?: { cpp_standard?: CppStandard };
  benchmark_config?: BenchmarkConfig;
}

export interface SourceLocation {
  file: string;
  line: number;
  column: number;
  end_line?: number;
  end_column?: number;
}

export interface Workload {
  status: BoundStatus;
  upper_bound: number | null;
  method: string | null;
  assumptions: string[];
  missing_bounds: string[];
}

export interface Quantity {
  expression: string;
  asymptotic: string | null;
  variables: string[];
  workload: Workload;
}

export interface Finding {
  id: string;
  category: 'loop_nest' | 'recursion' | 'static_storage' | 'dynamic_allocation' | 'unsupported_construct';
  pattern: string;
  description: string;
  function: string;
  location: SourceLocation;
  time: Quantity | null;
  space: Quantity | null;
  bound_status: BoundStatus;
  confidence: Confidence;
  assumptions: string[];
  limitations: string[];
  provenance: string;
}

export interface Assessment {
  status: FeasibilityStatus;
  reason: string;
}

export interface BenchmarkSample {
  size: Record<string, number>;
  wall_time_median_ms: number;
  wall_time_mad_ms: number;
  peak_rss_bytes: number;
  raw_samples_ms: number[];
}

export interface EmpiricalFit {
  model: string | null;
  r_squared: number;
  rss: number;
  slope: number;
  intercept: number;
  ambiguous: boolean;
}

export interface ExtrapolatedRuntime {
  target_size: Record<string, number>;
  min_ms: number;
  central_ms: number;
  max_ms: number;
  extrapolated: boolean;
}

export interface BenchmarkData {
  isolated: boolean;
  isolation_level: string;
  samples: BenchmarkSample[];
  empirical_fit: EmpiricalFit;
  extrapolated_runtime: ExtrapolatedRuntime;
  warnings: string[];
}

export interface AnalyzeResults {
  findings: Finding[];
  memory: {
    static_storage_bytes: number;
    static_storage_saturated: boolean;
    largest_local_array_bytes: number;
    estimated_minimum_bytes: number | null;
    estimated_minimum_status: BoundStatus;
    assumptions: string[];
    items: {
      name: string;
      kind: 'global_static' | 'local_array' | 'vector_buffer';
      type: string;
      exact_bytes: number | null;
      symbolic_bytes: string | null;
      location: SourceLocation;
    }[];
  };
  feasibility: { time: Assessment; memory: Assessment };
  summary: {
    finding_count: number;
    max_time_workload: number | null;
    dominant_finding_id: string | null;
    coverage: 'partial';
  };
  benchmark?: BenchmarkData | undefined;
}

export interface BenchmarkResults extends AnalyzeResults {
  benchmark: BenchmarkData;
}

export interface CapabilitiesResults {
  actions: string[];
  features: Record<string, string>;
  stl_cost_model: {
    container: string;
    operation: string;
    average: string;
    worst: string;
    amortized: boolean;
    note: string;
  }[];
}

export interface Diagnostic {
  severity: 'error' | 'warning' | 'information';
  message: string;
  location: SourceLocation | null;
}

export interface ProtocolError {
  code: string;
  message: string;
  path?: string;
}

export interface EngineResponse {
  schema_version: string;
  request_id: string | null;
  status: 'ok' | 'error';
  engine: { name: string; version: string; protocol_version: string; frontend: string | null };
  results: AnalyzeResults | BenchmarkResults | CapabilitiesResults | null;
  diagnostics: Diagnostic[];
  errors: ProtocolError[];
}

export function isAnalyzeResults(results: EngineResponse['results']): results is AnalyzeResults {
  return results !== null && 'findings' in results;
}

export function isBenchmarkResults(results: EngineResponse['results']): results is BenchmarkResults {
  return isAnalyzeResults(results) && results.benchmark !== undefined;
}

export function isCapabilitiesResults(results: EngineResponse['results']): results is CapabilitiesResults {
  return results !== null && 'features' in results;
}
