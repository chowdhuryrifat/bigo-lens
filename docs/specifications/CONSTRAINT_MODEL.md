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

When the expression is a monotone product of the constrained variables, the workload is bounded by substituting the
product constraint:

- For exact product expressions (`n · m`), the bound is `min(P, Π Mᵢ)`, reported with method
  `product-constraint-bound` and status `proven`.
- For expressions with higher powers of constrained variables (e.g. `n² · m`), the product `n · m` is bounded by `P`,
  with remaining factors evaluated at their individual maxima (e.g. `P · n_max`), method `product-constraint-bound`,
  with assumption "workload bounded by product constraint with remaining factors evaluated at individual maxima".
- For non-monotone expressions or expressions where the product variables are not multiplied together (e.g. `n + m`),
  the evaluation falls back to pointwise maxima with the assumption "product constraint (bound P) not used to tighten
  the bound; result may be loose".

## 6. Linear constraints

`{ "coefficients": { "n": 1, "m": 2 }, "bound": B }` states `Σ cᵢ · xᵢ ≤ B` (at least one term, all variables
declared, values within 10¹⁸).

Linear constraints with non-negative coefficients are used to tighten workloads for monotone expressions:
- **Variable bound tightening.** Before expression evaluation, individual variable upper bounds are tightened
  iteratively using the minima of other terms: `M_v' = min(M_v, floor((B - Σ_{u != v} c_u · m_u) / c_v))`.
- **Linear sums.** Linear combinations `Σ αᵢ · xᵢ` within addition expressions are bounded by `max_i(αᵢ / cᵢ) · B_eff + nonLinSum`.
- **Constrained optimization on products.** For monotone products `x^{a_x} · y^{a_y}` subject to `c_x · x + c_y · y ≤ B`,
  the objective function along the constraint boundary is log-concave. The unconstrained peak at
  `x* = (a_x / (a_x + a_y)) · (B_eff / c_x)` is clamped to the feasible interval `[x_lo, x_hi]`, where
  `x_lo = max(m_x, (B_eff - c_y · M_y) / c_x)` and `x_hi = min(M_x, (B_eff - c_y · m_y) / c_x)`.
  For higher-variable products, symmetric equal-fraction allocation provides a sound upper bound.
- When a linear constraint tightens the bound, the evaluation method is reported as `linear-constraint-bound` with
  status `proven`.
- Negative coefficients are out of scope for upper bound tightening; linear constraints with any negative coefficient
  are ignored for tightening and emit the assumption "linear constraint with negative coefficient (bound B) not used
  to tighten the bound; result may be loose".
- For non-monotone expressions, linear constraints emit the assumption "linear constraint (bound B) not used to tighten
  the bound on non-monotone expression; result may be loose".

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
3. **Product and linear constraints.** For monotone expressions, variable bounds are first tightened using linear
   constraints, and both product and linear constraints are evaluated during expression evaluation. If applied and
   tightening, the method is `product-constraint-bound` or `linear-constraint-bound`. When a product or linear constraint
   cannot be applied or the expression is non-monotone, an assumption ("product constraint ... not used" or
   "linear constraint ... not used") is emitted (§5, §6).
4. **No sum constraint on any free variable** → evaluate with product and linear constraints if applicable, otherwise at the
   maxima: method `product-constraint-bound`, `linear-constraint-bound`, or `pointwise-maxima`.
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

Applies when the expression is monotone and either:
1. **Single sum constraint.** Exactly one applicable sum constraint, and `perCase` is **superadditive** in the
   constrained variable `n` — i.e. f(a) + f(b) ≤ f(a + b), so concentrating the total into as few, as large cases
   as possible is the worst case.
2. **Multiple separable sum constraints.** Multiple sum constraints sharing the same count variable `t`, where
   `perCase` can be decomposed into a sum of additive terms `T_1 + T_2 + ... + T_k`, such that each term `T_j`
   depends on at most one sum-constrained variable. Each term is then evaluated and packed independently under its
   own sum constraint.

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

Other variables are evaluated at their maxima. For single sum constraints, the assumption "worst case packs <full>
test case(s) of maximum size (superadditive per-case cost)" is attached; for separable multi-sum constraints, "worst
case packs test case(s) of maximum size for sum-constrained variables (separable per-case cost)".

### 9.3 Fallback (`sum-constraint-cases`)

When packing does not apply (non-separable per-case cost where multiple sum-constrained variables interact, e.g. `n·m`,
per-case cost not recognised as superadditive, or a non-monotone expression):

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

### Example D — linear constraint: n, m ≤ 10⁶, n + m ≤ 2·10⁵, nested loop O(n · m)

Fixture `tests/fixtures/cpp/12_linear_constrained_loops.cpp`:

```cpp
for (int i = 0; i < n; ++i) {
  for (int j = 0; j < m; ++j) {
    acc += i ^ j;
  }
}
```

Expression: `n · m`. Variable upper bounds $10^6$. Linear constraint $n + m \le 200000$.

| Method | Computation | Result |
| --- | --- | --- |
| Naive (pointwise maxima) | $10^6 \times 10^6$ | $10^{12}$ — loose by 100× |
| Linear constraint bound | Optimization on $x + y = 2\cdot 10^5 \implies x^* = 10^5, y^* = 10^5$ | **$10^{10}$** |

Reported: `method: "linear-constraint-bound"`, `upper_bound: 1e10`, `status: "proven"`.

### Example E — multiple sum constraints: t ≤ 10⁴, n, m ≤ 2·10⁵, Σn ≤ 2·10⁵, Σm ≤ 3·10⁵, separable loop O(t · (n + m))

Fixture `tests/fixtures/cpp/13_multi_sum_constrained.cpp`:

```cpp
while (t--) {
  for (int i = 0; i < n; ++i) acc += i;
  for (int j = 0; j < m; ++j) acc += j;
}
```

Expression: `t · (n + m)`. Per-case: `n + m` (separable sum of two linear terms).

| Method | Computation | Result |
| --- | --- | --- |
| Naive (pointwise maxima) | $10^4 \times (2\cdot 10^5 + 2\cdot 10^5)$ | $4\cdot 10^9$ — loose |
| Multi-sum separable packing | Pack $n$ under $\Sigma n \le 2\cdot 10^5$ ($2\cdot 10^5$) + pack $m$ under $\Sigma m \le 3\cdot 10^5$ ($3\cdot 10^5$) | **$5\cdot 10^5$** |

Reported: `method: "sum-constraint-packing"`, `upper_bound: 500000`, `status: "proven"`.

---

## 11. What is used today

| Relationship | Parsed and validated | Used to tighten workload | Notes |
| --- | --- | --- | --- |
| Variable max | Yes | Yes | Required for every free variable |
| Variable min | Yes | Partially | Sign check for monotonicity; case-count reduction in fallback |
| Sum constraint | Yes | **Yes** | Packing (single or multi-variable separable) or fallback; one count variable per expression |
| Product constraint | Yes | **Yes** | Tightens monotone products; method `product-constraint-bound` |
| Linear constraint | Yes | **Yes** | Tightens monotone products, sums, and variable bounds; method `linear-constraint-bound` |
| Time limit | Yes | — | Feasibility only |
| Memory limit | Yes | — | Feasibility only |

Memory: sized-vector byte expressions are evaluated with `evaluateWorkload` under
`singleInstanceConstraints(constraints)`: each sum-constrained variable is capped at min(max, total) and the sum
relation is dropped, so one live buffer is sized and per-case allocations are not added up across test cases ([FEASIBILITY_MODEL.md §4](FEASIBILITY_MODEL.md#4-memory-feasibility)).
