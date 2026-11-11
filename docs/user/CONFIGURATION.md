# Configuration

BigO Lens is configured in two places:

1. **VS Code settings**, which control how the extension runs the engine
2. **A per-file constraints file**, which describes the problem constraints for one source file

## 1. VS Code settings

| Setting | Type | Default | Description |
| --- | --- | --- | --- |
| `bigoLens.enginePath` | string | `""` | Absolute path to the `bigolens-engine` executable. Leave it empty to search automatically (see below) |
| `bigoLens.cppStandard` | `c++11` \| `c++14` \| `c++17` \| `c++20` \| `c++23` | `c++20` | Language standard used to parse the source |
| `bigoLens.analysisTimeoutMs` | number (1000–600000) | `30000` | How long to wait for the engine before terminating it |

Example user `settings.json`:

```json
{
  "bigoLens.enginePath": "/home/you/src/bigo-lens/build/release/engine/bigolens-engine",
  "bigoLens.cppStandard": "c++17",
  "bigoLens.analysisTimeoutMs": 60000
}
```

### Engine location

When `bigoLens.enginePath` is empty, the extension uses the first of these that exists:

1. The `BIGOLENS_ENGINE_PATH` environment variable, as seen by VS Code
2. A bundled engine at `<extension>/bin/bigolens-engine` (`.exe` on Windows). Current packages do not include one yet
3. A development build in the repository: `build/dev`, `build/release`, `build/ci` or `build/no-clang`, under `engine/` (or `engine/Debug/` and `engine/Release/` for multi-config generators)

Rules:

- `bigoLens.enginePath` must be an **absolute** path. A relative path is rejected.
- If `bigoLens.enginePath` is set but unusable, the extension reports an error. It does **not** fall back to the other locations.
- The engine is run directly (never through a shell), with no command-line arguments. The request is sent on standard input.

### Engine environment variables

These are read by the **engine** process, which inherits VS Code's environment:

| Variable | Purpose |
| --- | --- |
| `BIGOLENS_CLANG_RESOURCE_DIR` | Overrides the Clang resource directory (builtin headers such as `stddef.h`) chosen at build time. See [TROUBLESHOOTING.md §2](../development/TROUBLESHOOTING.md#2-stddefh-not-found-or-missing-resource-directory) |

## 2. Constraints file

### 2.1 Location and lifecycle

- For a source file `solution.cpp`, the constraints file is **`solution.cpp.bigolens.json`** in the same directory.
- **BigO Lens: Edit Constraints for Active File** creates it from a template if it does not exist (it never overwrites an existing file), then opens it.
- **BigO Lens: Analyze Active C++ File** reads it if present. If it is **missing**, the analysis runs without constraints: expressions and O() bounds are still reported, but workloads are `unknown` and list their `missing_bounds`. If it is **invalid**, the analysis stops, and the errors are printed in the **BigO Lens** output channel.
- The file must be at most 256 KiB. It is validated against the `constraints` definition in [`shared/schemas/request.schema.json`](../../shared/schemas/request.schema.json) before use, and validated again by the engine.
- Constraints for unsaved (untitled) editors are not supported.

### 2.2 Format

The file contains exactly the `constraints` object of the [JSON protocol](../specifications/JSON_PROTOCOL.md) request. Unknown fields are rejected, which catches typos such as `"maximum"` instead of `"max"`.

| Field | Type | Meaning | Used today |
| --- | --- | --- | --- |
| `variables` | object: `name → { "min": int, "max": int }` | Bounds of each input variable | Yes. `max` is required for a workload. `min` is used in the monotonicity checks |
| `sum_constraints` | array of `{ "variable", "count_variable", "max_total" }` | "The sum of `variable` over all `count_variable` test cases is at most `max_total`" (Σn ≤ S) | Yes (packing, or a sound fallback) |
| `product_constraints` | array of `{ "variables": [names, at least 2], "max_product" }` | Product of the variables is at most `max_product` | Yes. Tightens monotone products (method `product-constraint-bound`) |
| `linear_constraints` | array of `{ "coefficients": { name: int }, "bound": int }` | Σ coefficientᵢ · variableᵢ ≤ bound | Yes. Tightens monotone products, sums, and variable bounds (method `linear-constraint-bound`) |
| `time_limit_ms` | integer, 1–3,600,000 | Time limit per test | Feasibility. Time is `unknown` until benchmarking exists |
| `memory_limit_mb` | integer, 1–1,048,576 | Memory limit | Memory feasibility |

Every field is optional, but a workload can only be computed when every variable in the expression has a `max`.

**Variable names**

- A variable name is a C++ identifier of at most 64 characters, optionally followed by `.size()`: `n`, `q`, `edges`, `v.size()`.
- Variables are matched **by name** to identifiers in the source. A constraint on `n` binds a loop bound written as `n`. A constraint on `v.size()` binds `v.size()` in a loop condition, and a range-for over `v`.
- If your code uses a different name for the same quantity, for example `cin >> len` where the statement calls it `n`, write the constraint with the name used **in the code** (`len`).

**Numbers**

- Integers with magnitude at most 10^18.
- Floats with an exact integral value are accepted, so `2e5` is the same as `200000`. `2.5` is rejected.

**Validation rules**, beyond types:

- `min` must not exceed `max`.
- Both the `variable` and the `count_variable` of a sum constraint must be declared with an upper bound, and must be different.
- A variable may have at most one sum constraint.
- Product and linear constraints may reference only declared variables.

### 2.3 Full example

This example describes a problem with *t* test cases, each with an array of length *n*, a grid of size *h × w*, and *q* queries:

```json
{
  "variables": {
    "t": { "min": 1, "max": 10000 },
    "n": { "min": 1, "max": 200000 },
    "q": { "min": 1, "max": 200000 },
    "h": { "min": 1, "max": 1000 },
    "w": { "min": 1, "max": 1000 },
    "a.size()": { "min": 1, "max": 200000 }
  },
  "sum_constraints": [
    { "variable": "n", "count_variable": "t", "max_total": 200000 },
    { "variable": "q", "count_variable": "t", "max_total": 200000 }
  ],
  "product_constraints": [
    { "variables": ["h", "w"], "max_product": 1000000 }
  ],
  "linear_constraints": [
    { "coefficients": { "n": 1, "q": 1 }, "bound": 300000 }
  ],
  "time_limit_ms": 2000,
  "memory_limit_mb": 256
}
```

How the current engine uses it:

- A `while (t--)` loop containing an O(n²) nest is evaluated with **sum-constraint packing**: ⌊200000 / 200000⌋ = 1 full case of n = 2·10^5, giving 4·10^10.
- Multiple sum constraints sharing count variable `t` (e.g. `Σn ≤ 2·10^5` and `Σq ≤ 2·10^5`) are evaluated via separable packing for additive shapes like `t·(n + q)`, giving 4·10^5 (`sum-constraint-packing`, `proven`). Interacting shapes (such as `n·q`) fall back soundly to `sum-constraint-cases`. Different count variables report `unknown`.
- An `h × w` grid loop is evaluated using the product constraint (`product-constraint-bound`, 10^6). A `h² · w` expression is tightened by substituting `h · w ≤ 10^6`, evaluating to `h_max · 10^6`.
- A nested loop or product such as `n × q` under `n + q ≤ 3·10^5` is tightened via constrained optimization on the line segment, evaluating to 2.25·10^10 (`linear-constraint-bound`). Sums `n + q` are bounded by 3·10^5. Single variable bounds are capped by `B - Σ c_u · m_u`.

### 2.4 Minimal examples

Single-test, one variable:

```json
{ "variables": { "n": { "min": 1, "max": 100000 } }, "time_limit_ms": 1000, "memory_limit_mb": 256 }
```

Container-size bound, for `for (auto x : v)` or `for (i < v.size())`:

```json
{ "variables": { "v.size()": { "min": 0, "max": 1000000 } } }
```

## 3. Settings that do not exist yet

The following are **planned** and do not work today: analysis on save, a constraint form UI, a webview report, and benchmark settings. See [ROADMAP.md](../development/ROADMAP.md).
