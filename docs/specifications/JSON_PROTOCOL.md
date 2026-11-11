# JSON Protocol Reference (version 1.0)

The protocol between clients (the VS Code extension, scripts, tests) and `bigolens-engine`. The normative definitions
are the JSON Schemas (draft 2020-12):

- [`shared/schemas/request.schema.json`](../../shared/schemas/request.schema.json)
- [`shared/schemas/response.schema.json`](../../shared/schemas/response.schema.json)

This document explains them and specifies behaviour the schemas cannot express. If this document and the schemas
disagree, the schemas win and this document is a bug.

Example requests: [`shared/examples/`](../../shared/examples/) — `capabilities.request.json`,
`analyze.request.json`, `benchmark.request.json`, `invalid.request.json`, `malformed.request.txt`.

Related: [ARCHITECTURE.md](../../ARCHITECTURE.md) · [DATA_FLOW.md](../architecture/DATA_FLOW.md) ·
[THREAT_MODEL.md](../engineering/THREAT_MODEL.md)

---

## 1. Transport and framing

| Aspect | Specification |
| --- | --- |
| Process model | One engine process per request. The client spawns `bigolens-engine` |
| Invocation | `bigolens-engine` (request on stdin) or `bigolens-engine --request FILE`. Other flags: `--version`, `--help` / `-h`. The VS Code extension passes **no arguments** and uses `shell: false` |
| Request | Exactly one JSON document (UTF-8) on stdin or in `FILE`, read until EOF |
| Response | Exactly one JSON object serialised on a **single line**, terminated by `\n`, on stdout |
| stderr | Free-form human-readable logs (e.g. internal error details). Not part of the protocol |
| Exit status | `0` whenever a response was written, **including error responses**; `2` usage error (unknown argument; no response); `1` other failures (e.g. request file cannot be opened, stdout write failure; no response) |
| Clients | Must read stdout to EOF, enforce their own timeout and output cap (extension: 16 MiB, `bigoLens.analysisTimeoutMs` default 30 000 ms), and treat a missing or invalid response as a client-side error |

### 1.1 Limits (checked by the engine)

| Limit | Value | Error |
| --- | --- | --- |
| Request size | 4 MiB (4 194 304 bytes) | `request_too_large` |
| JSON nesting depth | 64 levels, checked **before** parsing | `malformed_json` |
| `source.content` | 2 MiB (2 097 152) — the engine counts UTF-8 bytes; the schema's `maxLength` counts characters | `invalid_request` |
| `source.path` | 4096 | `invalid_request` |
| Integers | magnitude ≤ 10¹⁸ | `invalid_request` |
| Constraint arrays | ≤ 64 items each | `invalid_request` |
| Compiler error diagnostics | at most 50 collected | — |

---

## 2. Versioning fields

- Requests carry `schema_version` as `"MAJOR.MINOR"`.
- The engine accepts a request when **MAJOR equals its own and MINOR ≤ its own**. The engine at protocol 1.0 accepts
  only `"1.0"`. Otherwise: `unsupported_schema_version` with `path: "/schema_version"`.
- A `schema_version` not of the form `MAJOR.MINOR` is `invalid_request`.
- Responses carry the engine's own protocol version in `schema_version` and in `engine.protocol_version`.

---

## 3. Request

```json
{
  "schema_version": "1.0",
  "request_id": "example-analyze-sum-constraint",
  "action": "analyze",
  "source": { "path": "solution.cpp", "language": "cpp", "content": "int main() { ... }" },
  "constraints": { "variables": { "n": { "min": 1, "max": 200000 } }, "time_limit_ms": 2000 },
  "options": { "cpp_standard": "c++20" }
}
```

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `schema_version` | string `^[0-9]+\.[0-9]+$` | yes | See §2 |
| `request_id` | string `^[A-Za-z0-9_.:-]{1,128}$` | yes | Client-chosen; echoed in the response |
| `action` | `"capabilities"` \| `"analyze"` \| `"benchmark"` | yes | Operation |
| `source` | object | for `analyze`, `benchmark` | §3.1 |
| `constraints` | object | no | §4.2 |
| `options` | object | no | §3.2 |

**Unknown fields are rejected** at every level (`additionalProperties: false`), with `invalid_request` and the
JSON pointer of the offending member (e.g. `/unexpected`).

### 3.1 `source`

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `path` | string, ≤ 4096 | yes | Display path used verbatim in reported locations. The engine does **not** read this path; it analyses `content` as an in-memory file |
| `language` | `"cpp"` | yes | Only C++ is supported |
| `content` | string, ≤ 2 MiB | yes | The complete translation unit. Headers named in `#include` are resolved from the local filesystem as a compiler would |

### 3.2 `options`

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `cpp_standard` | `"c++11"` \| `"c++14"` \| `"c++17"` \| `"c++20"` \| `"c++23"` | `"c++20"` | The only request value that reaches compiler arguments |

---

## 4. Shared definitions

### 4.1 Integers and variable names

- **integer:** JSON number, `multipleOf: 1`, within ±10¹⁸. Floats with an exact integral value (`2e5`) are accepted.
- **nonNegativeInteger:** integer ≥ 0.
- **variableName:** `^[A-Za-z_][A-Za-z0-9_]{0,63}(\.size\(\))?$` — a C++ identifier, optionally followed by
  `.size()`. Matched by name to source identifiers ([CONSTRAINT_MODEL.md §2](CONSTRAINT_MODEL.md#2-variable-binding)).

### 4.2 Constraints

| Field | Type | Description |
| --- | --- | --- |
| `variables` | object: variableName → `{ "min"?: integer, "max"?: integer }` | Per-variable bounds |
| `sum_constraints` | array (≤ 64) of `{ "variable", "count_variable", "max_total": nonNegativeInteger }` | Σ `variable` over `count_variable` cases ≤ `max_total` |
| `product_constraints` | array (≤ 64) of `{ "variables": [variableName, …] (≥ 2), "max_product": nonNegativeInteger }` | Π variables ≤ `max_product` |
| `linear_constraints` | array (≤ 64) of `{ "coefficients": { variableName: integer } (≥ 1), "bound": integer }` | Σ cᵢ·xᵢ ≤ `bound` |
| `time_limit_ms` | integer 1 … 3 600 000 | Time limit |
| `memory_limit_mb` | integer 1 … 1 048 576 | Memory limit in MiB |

Semantic validation performed by the engine beyond the schema (`invalid_request`, `path: "/constraints"`):

- `min ≤ max` for every variable.
- Sum constraints: `variable` and `count_variable` both have a declared `max`; they differ; at most one sum
  constraint per `variable`.
- Product and linear constraints reference only declared variables.

Semantics: [CONSTRAINT_MODEL.md](CONSTRAINT_MODEL.md).

---

## 5. Response

```json
{
  "schema_version": "1.0",
  "request_id": "example-invalid",
  "status": "error",
  "engine": { "name": "bigolens-engine", "version": "0.1.0", "protocol_version": "1.0", "frontend": null },
  "results": null,
  "diagnostics": [],
  "errors": [{ "code": "invalid_request", "message": "unknown field 'unexpected'", "path": "/unexpected" }]
}
```

(Response to `shared/examples/invalid.request.json` from an engine built without the Clang frontend; with the
frontend, `engine.frontend` holds the Clang version string.)

| Field | Type | Description |
| --- | --- | --- |
| `schema_version` | string | Engine's protocol version |
| `request_id` | string \| null | Echoed id; `null` if it could not be recovered (malformed JSON, oversize, invalid id) |
| `status` | `"ok"` \| `"error"` | `error` ⇔ `errors` non-empty; `ok` ⇔ `errors` empty |
| `engine` | object | `name`, `version`, `protocol_version`, `frontend` (string or `null` if built with `BIGOLENS_ENABLE_CLANG=OFF`) |
| `results` | object \| null | Action-specific (§6); `null` on error |
| `diagnostics` | array | `{ severity: "error" \| "warning" \| "information", message, location: Location \| null }`. Compiler diagnostics for the analysed source |
| `errors` | array | `{ code, message, path? }` (§7) |

All top-level fields are always present.

**Location:** `{ file, line, column, end_line?, end_column? }`, integers ≥ 0. Lines and columns are 1-based (Clang
presumed locations, mapped through macro expansion); `0` means unknown. `end_column` is the column of the start of
the last token. `file` is `source.path`.

---

## 6. Results by action

### 6.1 `capabilities`

| Field | Type | Description |
| --- | --- | --- |
| `actions` | string[] | `["capabilities", "analyze", "benchmark"]` |
| `features` | object: name → `partial` \| `unsupported` \| `data_only` \| `requires_runtime_estimate` \| `not_implemented` \| `supported` | Support level per feature |
| `stl_cost_model` | array of `{ container, operation, average, worst, amortized: boolean, note }` | Cost table |

Features reported by engine 0.1.0:

| Feature | Value |
| --- | --- |
| `loop_analysis` | `partial` |
| `while_countdown_loops` | `partial` |
| `range_for_loops` | `partial` |
| `recursion_analysis` | `partial` (Master Theorem and linear decrements) |
| `recurrence_extraction` | `partial` (direct recursion) |
| `stl_cost_model` | `partial` (applied to recognized container operations and top-level algorithms) |
| `interprocedural_analysis` | `partial` (direct non-recursive calls in the same translation unit) |
| `constraint_workload` | `partial` |
| `static_memory_estimate` | `partial` |
| `time_feasibility` | `requires_runtime_estimate` |
| `benchmark` | `not_implemented` |

`capabilities` works without the Clang frontend. Clients must tolerate feature names they do not know.

### 6.2 `analyze`

```
results
├─ findings[]     Finding
├─ memory         Memory
├─ feasibility    { time: Assessment, memory: Assessment }
└─ summary        { finding_count, max_time_workload, dominant_finding_id, coverage }
```

**Finding** (all fields required):

| Field | Type | Description |
| --- | --- | --- |
| `id` | string | `F1`, `F2`, … in source order |
| `category` | `loop_nest` \| `recursion` \| `static_storage` \| `dynamic_allocation` \| `unsupported_construct` | `unsupported_construct` is reserved; not emitted by engine 0.1.0 |
| `pattern` | string | See [COMPLEXITY_MODEL.md §4](COMPLEXITY_MODEL.md#4-patterns). Clients must tolerate unknown patterns |
| `description` | string | Human-readable |
| `function` | string | Enclosing function name; not meaningful for namespace-scope storage findings |
| `location` | Location | Source range |
| `time` | Quantity \| null | Structural count of innermost-body executions; `null` when unknown/unsupported or not applicable |
| `space` | Quantity \| null | Bytes, for storage findings |
| `bound_status` | `proven` \| `estimated` \| `unknown` \| `unsupported` | Kind of claim |
| `confidence` | `high` \| `medium` \| `low` \| `none` | Derived from status (and, for loops, limitations and early exit); see [ANALYSIS_MODEL.md §6](../design/ANALYSIS_MODEL.md#6-bound-status-and-confidence) |
| `assumptions` | string[] | Conditions under which the result holds |
| `limitations` | string[] | What is not included or why it failed |
| `provenance` | string | `clang-loop-analyzer` \| `clang-recursion-detector` \| `clang-storage-analyzer` |

**Quantity:**

| Field | Type | Description |
| --- | --- | --- |
| `expression` | string | Human-readable rendering; not a parse format |
| `asymptotic` | string \| null | `O(...)` upper bound or `null` |
| `variables` | string[] | Free variables |
| `workload` | Workload | Evaluation under constraints |

**Workload:**

| Field | Type | Description |
| --- | --- | --- |
| `status` | boundStatus | `proven`, `estimated` or `unknown` |
| `upper_bound` | number \| null | Upper bound of the quantity under constraints |
| `method` | string \| null | `pointwise-maxima`, `sum-constraint-packing`, `sum-constraint-cases`, `pointwise-maxima-capped`, `product-constraint-bound`, `linear-constraint-bound` |
| `assumptions` | string[] | |
| `missing_bounds` | string[] | Variables without `max` |

**Memory:**

| Field | Type | Description |
| --- | --- | --- |
| `static_storage_bytes` | integer ≥ 0 | Exact sum of global/static storage; proven lower bound |
| `static_storage_saturated` | boolean | Sum overflowed 64 bits and was clamped |
| `largest_local_array_bytes` | integer ≥ 0 | Largest constant-size local array |
| `estimated_minimum_bytes` | number \| null | Static storage + sized vectors, each at its largest single-instance size under the constraints; `null` if a vector size could not be evaluated |
| `estimated_minimum_status` | boundStatus | `proven` (no sized vectors), `estimated` (sized vectors evaluated), `unknown` |
| `assumptions` | string[] | |
| `items[]` | `{ name, kind: global_static \| local_array \| vector_buffer, type, exact_bytes: integer \| null, symbolic_bytes: string \| null, location }` | Storage items |

**Assessment:** `{ status: likely_safe | borderline | high_risk | unknown, reason: string }`
([FEASIBILITY_MODEL.md](FEASIBILITY_MODEL.md)).

**Summary:**

| Field | Type | Description |
| --- | --- | --- |
| `finding_count` | integer | Number of findings |
| `max_time_workload` | number \| null | Largest `time.workload.upper_bound` among findings |
| `dominant_finding_id` | string \| null | Finding with that workload |
| `coverage` | `"partial"` | Always `partial` in 1.0 |

Abridged `analyze` result for `shared/examples/analyze.request.json` (values asserted by
`tests/integration/frontend_pipeline_test.cpp`; strings and arrays shortened). The complete response, produced by
the engine with `scripts/development/regenerate-examples.sh`, is
[`shared/examples/analyze.response.json`](../../shared/examples/analyze.response.json):

```jsonc
{
  "findings": [{
    "id": "F1", "category": "loop_nest", "pattern": "test-case-loop", "function": "main",
    "time": {
      "workload": { "status": "proven", "upper_bound": 40000000000.0, "method": "sum-constraint-packing", "…": "…" },
      "…": "…"
    },
    "…": "…"
  }],
  "summary": { "max_time_workload": 40000000000.0, "dominant_finding_id": "F1", "coverage": "partial", "…": "…" },
  "…": "…"
}
```

Run `bigolens-engine --request shared/examples/analyze.request.json` to obtain the complete response.

### 6.3 `benchmark`

Not implemented (M4). Always returns `status: "error"` with `not_implemented` after the request validates (the
`source` member is required).

---

## 7. Error codes

| Code | When | `request_id` | `path` | `results` |
| --- | --- | --- | --- | --- |
| `malformed_json` | Not valid JSON, or nesting > 64 | `null` | — | `null` |
| `request_too_large` | Request > 4 MiB | `null` | — | `null` |
| `invalid_request` | Schema or semantic validation failure; **all** problems found are reported, one error each | echoed if valid | JSON pointer | `null` |
| `unsupported_schema_version` | Major differs or minor newer than the engine's | echoed if valid | `/schema_version` | `null` |
| `not_implemented` | `benchmark` action | echoed | — | `null` |
| `frontend_unavailable` | `analyze` on an engine built with `BIGOLENS_ENABLE_CLANG=OFF` | echoed | — | `null` |
| `source_parse_error` | The source has compile errors; `diagnostics` contains them (≤ 50 errors). Analysis never runs on an AST with errors | echoed | — | `null` |
| `internal_error` | Unexpected exception inside the engine; details on stderr | echoed | — | `null` |

Messages are human-readable and may change in any version; clients must branch on `code`, not `message`.

---

## 8. Compatibility and versioning

| Change | Version bump | Examples |
| --- | --- | --- |
| Additive | **Minor** | New optional request field, new response field, new action, new enum value in an open-ended string field (`pattern`, `method`, feature names) |
| Removal or semantic change | **Major** | Removing or renaming a field, changing a type, changing the meaning of a status, making an optional request field required, adding a value to a closed response enum (`status`, `bound_status`, `category`, error `code`) |

Rules:

1. **Engine:** accepts the same major and minor ≤ its own (§2). It **rejects unknown request fields**, so a client
   must not send fields introduced in a newer minor version to an older engine. A client that wants to use a newer
   feature first checks `engine.protocol_version` via `capabilities`.
2. **Extension:** validates every response **strictly** against the `response.schema.json` it ships with (copied
   from `shared/schemas/` at build time). Because the response schema uses `additionalProperties: false`, a
   response containing fields from a newer minor version fails validation. Engine and extension are therefore
   released together and the bundled/expected engine must match the extension's protocol version. Strings in
   open-ended fields (`pattern`, `method`, `assumptions`, feature names) are displayed as-is and must not be relied
   on for logic beyond known values.
3. Both schemas are versioned with the protocol; any schema change updates this document, the examples, and the
   protocol constants (`kProtocolMajor`, `kProtocolMinor` in `engine/src/version.hpp.in`) in the same change.

See [ADR records](../architecture/DECISIONS/) for the rationale.

---

## 9. Validation at boundaries

| Boundary | Validator | Behaviour |
| --- | --- | --- |
| Sidecar constraints file → extension | ajv against the `constraints` definition of the request schema | Invalid file blocks the request; errors shown to the user |
| Extension → engine (request) | Engine `parseRequest` + `constraints::validate` | Structured `invalid_request` errors with JSON pointers; no partial processing |
| Raw bytes → engine | Size and depth checks before JSON parsing | `request_too_large`, `malformed_json` |
| Engine → extension (response) | ajv against the shipped `response.schema.json` | A response that fails validation is treated as an engine failure and not rendered |
| Engine → Clang | Fixed argument list; only validated `cpp_standard` from the request; in-memory file; `-fsyntax-only`; warnings suppressed | — |
| CI | Schema-validation step in the extension job; integration tests against the real engine | — |

The engine's hand-written validation mirrors the request schema; where they differ in detail (e.g. byte vs character
length of `content`), a request must satisfy both.
