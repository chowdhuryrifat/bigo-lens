# Constraint Model

Specification of the constraints BigO Lens accepts, how they bind to source code, and how they turn a symbolic
structural count into a workload upper bound. Implementation: `engine/src/constraints/constraint_set.cpp`
(validation) and `engine/src/constraints/workload.cpp` (`evaluateWorkload`). Milestone **M3**, status **partial**.

Related: [ANALYSIS_MODEL.md](../design/ANALYSIS_MODEL.md) · [COMPLEXITY_MODEL.md](COMPLEXITY_MODEL.md) ·
[JSON_PROTOCOL.md](JSON_PROTOCOL.md#42-constraints) · [FEASIBILITY_MODEL.md](FEASIBILITY_MODEL.md) ·
[CONFIGURATION.md](../user/CONFIGURATION.md)

---

## 1. Constraint set

```json
{
  "variables": { "t": { "min": 1, "max": 10 }, "n": { "min": 1, "max": 200000 } },
  "sum_constraints": [{ "variable": "n", "count_variable": "t", "max_total": 200000 }],
  "product_constraints": [{ "variables": ["n", "m"], "max_product": 1000000 }],
  "linear_constraints": [{ "coefficients": { "n": 1, "m": 2 }, "bound": 300000 }],
  "time_limit_ms": 2000,
  "memory_limit_mb": 256
}
```

All members are optional. Numeric values are integers of magnitude at most 10¹⁸; integral floating literals such as
`2e5` are accepted. Unknown members are rejected. Every array holds at most 64 items.

In the VS Code extension this object is stored in the sidecar file `<source file>.bigolens.json`
([UI_UX_DESIGN.md §4](../design/UI_UX_DESIGN.md#4-manual-constraint-configuration)).

---

## 2. Variable binding

Constraint variables are matched **by name** to the symbolic variables the analyzer creates from source code. There is
no declaration-level binding.

| Source construct used as a loop bound | Symbolic variable | Constraint key |
| --- | --- | --- |
| Integer variable `n` (local, parameter, global) | `n` | `"n"` |
| `v.size()` or `v.length()` on a named object `v` | `v.size()` | `"v.size()"` |
| Range-for over named container `v` | `v.size()` | `"v.size()"` |
| `while (t--)` counter `t` | `t` | `"t"` |
| `const` / `constexpr` integer with a known value | constant (no variable) | — |

Name rules: a C++ identifier of at most 64 characters, optionally followed by `.size()` (pattern
`^[A-Za-z_][A-Za-z0-9_]{0,63}(\.size\(\))?$`).

Consequences (see also [LIMITATIONS.md §12](LIMITATIONS.md#12-name-based-constraint-binding)):

- Two different declarations with the same name (shadowing, `n` in two functions) share one constraint.
- A constraint whose name matches no expression variable has no effect and produces no warning in protocol 1.0.
- A variable that appears in an expression but has no `max` is reported in `workload.missing_bounds`.
- Values read from data (e.g. range-for elements, `a[i]`) are not sizes and never become variables; loops bounded by
  them are `unknown`.

---

## 3. Upper and lower bounds

`variables.<name>.min` / `.max` define `min ≤ name ≤ max`. Either side may be omitted.

| Rule | Effect |
| --- | --- |
| `max` missing for a free variable of the expression | Workload `unknown`, name in `missing_bounds`, assumption "no upper bound configured for '<name>'" |
| `min` missing | Assumption "'<name>' assumed non-negative (no lower bound configured)" |
| `min < 0` for any free variable | Monotonicity cannot be relied on: status at most `estimated` |
| `min > max` | Rejected (`invalid_request`) |
| `min ≥ 1` on a sum-constrained variable | Tightens the case count in the `sum-constraint-cases` fallback ([§7.3](#93-fallback-sum-constraint-cases)) |

Lower bounds are not used to produce lower-bound workloads; workloads are always upper bounds.

---

## 4. Sum constraints

`{ "variable": "n", "count_variable": "t", "max_total": S }` states: over all `t` test cases, the sum of the per-case
values of `n` is at most `S` — the "sum of n over all test cases does not exceed S" clause.

Validation:

- Both `variable` and `count_variable` must have a declared `max`.
- `variable` ≠ `count_variable`.
- `max_total` ∈ [0, 10¹⁸].
- At most one sum constraint per `variable`.

Sum constraints are **used** to tighten workloads ([§7](#9-aggregate-workload-calculation)).

## 5. Product constraints

`{ "variables": ["n", "m"], "max_product": P }` states `n · m ≤ P` (at least two variables, all declared, `P` ∈
[0, 10¹⁸]).

**Validated only.** When at least two of its variables occur in an expression, the workload carries the assumption
"product constraint (bound P) not used to tighten the bound; result may be loose". The bound is then computed from
the individual maxima, which remains sound but may be far above the true worst case (e.g. `n·m` with `n, m ≤ 10⁶`,
`n·m ≤ 10⁶` evaluates to 10¹² instead of 10⁶).

## 6. Linear constraints

`{ "coefficients": { "n": 1, "m": 2 }, "bound": B }` states `Σ cᵢ · xᵢ ≤ B` (at least one term, all variables
declared, values within 10¹⁸).

**Validated only.** When any linear constraint is present, every workload carries the assumption "linear
constraints are not yet used to tighten workload bounds".

## 7. Multiple test cases

A test-case wrapper `while (t--)` produces an expression `t × perCase`. The count variable of a sum constraint is
normally that `t`. Per-case loops that are *not* inside the wrapper but use a sum-constrained variable are treated as
per-test-case costs and summed over all cases (assumption "cost treated as per test case because 'n' is constrained
across test cases ('t')"); this is conservative.

## 8. Time and memory limits

- `time_limit_ms` ∈ [1, 3 600 000].
- `memory_limit_mb` ∈ [1, 1 048 576]; the engine converts it to bytes as `mb × 1024 × 1024` (MiB).

They are used only by feasibility ([FEASIBILITY_MODEL.md](FEASIBILITY_MODEL.md)); they never change workloads.

---

## 9. Aggregate workload calculation

`evaluateWorkload(expr, constraints)` for one expression:

1. **Collect bounds.** Every free variable needs a `max`; otherwise `unknown` (stop).
2. **Monotonicity.** If the expression is syntactically monotone non-decreasing (assuming variables ≥ 0) and no free
   variable has a negative `min`, the status is `proven`; otherwise `estimated` with the assumption "expression is
   not provably non-decreasing; evaluated at variable maxima, which may not be the worst case".
3. **Unused relationships.** Add product/linear assumptions (§5, §6).
4. **No sum constraint on any free variable** → evaluate at the maxima: method `pointwise-maxima`.
5. **Sum constraints present:**
   - All applicable sum constraints must share one `count_variable`; otherwise `unknown` ("sum constraints over
     different test-case counts are not supported").
   - Each sum-constrained variable is **capped** at `min(max, max_total)` — no single case can exceed the total.
   - Separate the count factor: if the expression has the form `t × perCase` (with `t` or `max(0, t)` appearing
     exactly once as a factor and `perCase` independent of `t`), use `perCase`. If `t` appears in another shape,
     evaluate the whole expression with capped values and `t = t_max`: method `pointwise-maxima-capped`. If `t`
     does not appear, the whole expression is the per-case cost.
   - Then apply packing (§9.2) or the fallback (§9.3).
6. **Evaluation failure** (overflow, budget exceeded) → `unknown` with "evaluation failed: …".

Monotonicity rules (syntactic, variables assumed ≥ 0): constants and variables are monotone; sums of monotone terms
are monotone (constant terms of either sign allowed); products are monotone if every factor is monotone and
non-negative; `x^k` is monotone if `x` is monotone and non-negative; `log(x)` is monotone if `x` is; `max` of monotone
operands is monotone; a summation is monotone if its lower limit is constant and its upper limit and non-negative
body are monotone.

### 9.1 Why the naive product is wrong

The naive bound evaluates every variable at its own maximum and multiplies by the maximum number of cases. With a sum
constraint, those maxima cannot all occur simultaneously.

### 9.2 Sum-constraint packing (`sum-constraint-packing`)

Applies when there is exactly one applicable sum constraint, the expression is monotone, and `perCase` is
**superadditive** in the constrained variable `n` — i.e. f(a) + f(b) ≤ f(a + b), so concentrating the total into as
few, as large cases as possible is the worst case.

Recognised superadditive shapes: `n`; sums of superadditive terms (plus non-positive constants); products of monotone
non-negative factors where at least one factor is superadditive in `n` (`n²`, `n·m`, `n·log n`); `x^k` with `k ≥ 1`
and superadditive `x`; `max(c ≤ 0, f)`; summations `Σ_{i=0}^{n+c} g(i)` with `c ≤ −1` and `g ≥ 0` non-decreasing and
independent of `n` (e.g. the triangular `n(n−1)/2`). Logarithms and constants alone are not superadditive.

With `n_cap = min(n_max, S)` and `t_max` the count maximum:

```
full      = min(t_max, floor(S / n_cap))
remainder = S − full · n_cap
workload  = full · perCase(n_cap) + (full < t_max and remainder > 0 ? perCase(remainder) : 0)
```

Other variables are evaluated at their maxima. The assumption "worst case packs <full> test case(s) of maximum size
(superadditive per-case cost)" is attached.

### 9.3 Fallback (`sum-constraint-cases`)

When packing does not apply (per-case cost not recognised as superadditive, more than one sum constraint, or a
non-monotone expression):

```
cases    = t_max, reduced to floor(S / n_min) for each sum-constrained variable with n_min ≥ 1
workload = cases · perCase(capped values)
```

This is a sound upper bound for monotone per-case costs because every case costs at most `perCase(n_cap)`, but it can
be loose. Assumption: "per-case cost not recognised as superadditive; bound is cases x per-case maximum".

---

## 10. Worked examples

### Example A — T ≤ 10, N ≤ 2·10⁵, ΣN ≤ 2·10⁵, per-case O(N²)

Fixture `tests/fixtures/cpp/09_sum_constrained_test_cases.cpp`, request `shared/examples/analyze.request.json`:

```cpp
while (t--) {
  int n; std::cin >> n;
  for (int i = 0; i < n; ++i)
    for (int j = 0; j < n; ++j) acc += i * j;
}
```

Structural count: `t × n²`; per case `n²` (superadditive).

| Method | Computation | Result |
| --- | --- | --- |
| Naive (pointwise maxima, ignores ΣN) | 10 × (2·10⁵)² | 4·10¹¹ — **wrong** (overstates by 10×) |
| Packing | `n_cap` = min(2·10⁵, 2·10⁵) = 2·10⁵; `full` = min(10, ⌊2·10⁵ / 2·10⁵⌋) = 1; `remainder` = 0; 1 × (2·10⁵)² | **4·10¹⁰** |

Reported: `method: "sum-constraint-packing"`, `upper_bound: 4e10`, `status: "proven"`; the integration test asserts
this value and `summary.max_time_workload = 4e10`.

### Example B — remainder packing: t ≤ 100, n ≤ 1000, Σn ≤ 5500, per-case O(n²)

| Step | Value |
| --- | --- |
| `n_cap` | min(1000, 5500) = 1000 |
| `full` | min(100, ⌊5500 / 1000⌋) = 5 |
| `remainder` | 5500 − 5 · 1000 = 500 |
| Full cases | 5 · 1000² = 5·10⁶ |
| Remainder case | 500² = 2.5·10⁵ |
| **Workload** | 5·10⁶ + 2.5·10⁵ = **5.25·10⁶** |
| Naive | 100 · 1000² = 10⁸ |

### Example C — fallback for a non-superadditive per-case cost

Per case `for (int i = 1; i * i <= n; ++i)`: cost `n^0.5 + 1` (power < 1, not superadditive). With `t ∈ [1, 10⁴]`,
`n ∈ [1, 2·10⁵]`, Σn ≤ 2·10⁵:

`cases` = min(10⁴, ⌊2·10⁵ / 1⌋) = 10⁴; per-case maximum ≈ √(2·10⁵) + 1 ≈ 448.2; workload ≈ 4.48·10⁶ via
`sum-constraint-cases`. This is sound but loose (spreading the total evenly gives about 5.5·10⁴); tightening it for
concave costs is future work.

---

## 11. What is used today

| Relationship | Parsed and validated | Used to tighten workload | Notes |
| --- | --- | --- | --- |
| Variable max | Yes | Yes | Required for every free variable |
| Variable min | Yes | Partially | Sign check for monotonicity; case-count reduction in fallback |
| Sum constraint | Yes | **Yes** | Packing or fallback; one count variable per expression |
| Product constraint | Yes | No (M3 follow-up) | Assumption emitted |
| Linear constraint | Yes | No (M3 follow-up) | Assumption emitted |
| Time limit | Yes | — | Feasibility only |
| Memory limit | Yes | — | Feasibility only |

Memory: sized-vector byte expressions are evaluated with `evaluateWorkload` under
`singleInstanceConstraints(constraints)`: each sum-constrained variable is capped at min(max, total) and the sum
relation is dropped, so one live buffer is sized and per-case allocations are not added up across test cases ([FEASIBILITY_MODEL.md §4](FEASIBILITY_MODEL.md#4-memory-feasibility)).
