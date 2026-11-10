# Complexity Model

Specification of the symbolic expressions, loop forms, trip-count formulas, status rules, patterns, asymptotic
derivation and numeric evaluation used by the static analyzer. Milestone **M2**, status **partial**.

Implementation:

| Concern | File |
| --- | --- |
| Expression tree | `engine/include/bigolens/complexity/expression.hpp`, `engine/src/complexity/expression.cpp` |
| Numeric evaluator | `engine/include/bigolens/complexity/evaluator.hpp`, `engine/src/complexity/evaluator.cpp` |
| O() derivation | `engine/include/bigolens/complexity/asymptotic.hpp`, `engine/src/complexity/asymptotic.cpp` |
| Loop / recursion / storage analysis | `engine/src/parser/clang_frontend.cpp` (the only file including Clang headers) |

Related: [ANALYSIS_MODEL.md](../design/ANALYSIS_MODEL.md) · [CONSTRAINT_MODEL.md](CONSTRAINT_MODEL.md) ·
[LIMITATIONS.md](LIMITATIONS.md)

---

## 1. Expression tree

Expressions are immutable, shared trees (`std::shared_ptr<const Expr>`). Variables denote integers that are ≥ 0
unless constrained otherwise.

| Node | Semantics | Construction constraints |
| --- | --- | --- |
| `Constant(c)` | `c` (IEEE double) | — |
| `Variable(v)` | value bound to `v` (`n`, `v.size()`) | — |
| `Add(a, b, …)` | `a + b + …` | — |
| `Multiply(a, b, …)` | `a · b · …` | — |
| `Power(a, k)` | `a^k` | `k` constant, finite, `k ≥ 0` (else `std::invalid_argument`) |
| `Logarithm(a, base)` | `log_base(max(a, 1))` | `base > 1`, finite (else `std::invalid_argument`); default base 2 |
| `Summation(i, lo, hi, body)` | `Σ body` for integer `i ∈ [⌈lo⌉, ⌊hi⌋]`; 0 if empty | `i` is bound inside `body` |
| `Maximum(a, b, …)` | `max(a, b, …)` | — |

Factory functions (`constant`, `variable`, `add`, `subtract`, `multiply`, `power`, `logarithm`, `summation`,
`maximum`) apply **only semantics-preserving simplifications** (e.g. `n + 0 → n`, `n · 1 → n`). They never apply
simplifications that are valid only asymptotically.

`freeVariables(expr)` returns the free variables (summation indices are bound). `toString(expr)` renders a
human-readable form (e.g. `sum_{i=0}^{n - 1} i`, `4 * n`); it is **not** a parse format and may change between
minor versions.

---

## 2. Supported loop forms

The analyzer runs Clang with `-fsyntax-only` on an in-memory file. Per function definition in the main file
(template primaries only), each **top-level loop nest** yields one `loop_nest` finding.

| # | Form | Trip count | Pattern (single loop) |
| --- | --- | --- | --- |
| L1 | `for (T i = a; i < b; ++i)` (also `i++`, `i = a` init, `i != b` with unit step) | `(b − 1) − a + 1` | `linear-loop` |
| L2 | `for (T i = a; i <= b; ++i)` | `b − a + 1` | `linear-loop` |
| L3 | `for (…; i < b / i <= b; i += c)`, constant `c > 0` | `(last − a) · (1/c) + 1`, `last` = `b − 1` or `b` | `linear-loop` |
| L4 | `for (T i = a; i > b; --i)` (also `i != b` with unit step) | `a − (b + 1) + 1` | `linear-loop` |
| L5 | `for (T i = a; i >= b; --i)`; `i -= c` | `a − b + 1`; `(a − last) · (1/c) + 1` | `linear-loop` |
| L6 | `for (i = a; i < b / i <= b; i *= c)` or `i <<= k` (`a` constant ≥ 1) | `log_c(b) + 1` (`c = 2^k` for shifts, `1 ≤ k ≤ 30`) | `logarithmic-loop` |
| L7 | `for (i = a; i > 0 / i >= 1 / i != 0; i /= c)` or `i >>= k` | `log_c(a) + 1` | `logarithmic-loop` |
| L8 | `for (i = …; i * i <= b / i * i < b; ++i)` | `b^0.5 + 1` (index assumed to start ≥ 0) | `sqrt-loop` |
| L9 | `while (t--)`, `while (t-- > 0)` | `t` | `test-case-loop` |
| L10 | `while (x > 0)`, `while (x)`, `while (x != 0)`, `while (x >= 1)` with exactly one top-level update `x--`/`--x`/`x -= 1` (an unbraced body counts as one top-level statement) | `x` | `linear-loop` |
| L11 | Same conditions with exactly one update `x /= c` (`c ≥ 2`) or `x >>= k` (`1 ≤ k ≤ 30`) | `log_c(x) + 1` | `logarithmic-loop` |
| L12 | Range-for over a named container `v` | `v.size()` | `range-for-loop` |
| L13 | Range-for over a constant-size array | its size (constant) | `range-for-loop` |

Requirements common to `for` forms: single declared or assigned integer index; init, condition and increment all
present; index not written in the body.

**Bounds** (`a`, `b`, `x`) may be: integer constants (including `const`/`constexpr` variables with known values),
integer variables, `v.size()` / `v.length()` on a named object (both map to the variable `v.size()`), and
combinations with `+`, `−`, `*`, and `/` by a positive constant (real division, which over-approximates integer
division for non-negative operands). Anything else (array elements, function calls, range-for element values) makes
the loop `unknown`.

### 2.1 Nesting and composition

- A straight-line body costs `1`.
- A body containing loops costs the **sum** of its child loops (assumption "sequential inner loops are summed").
- If the body cost does not depend on the index: `cost = trips × body`.
- If it depends on the index and the loop has unit step (L1, L2, L4, L5 with `c = 1`): `cost = Σ_{i=lo}^{hi} body`
  over the inclusive index range, pattern `triangular-loop`. Example: `for (i = 0; i < n; ++i) for (j = 0; j < i; ++j)`
  gives `Σ_{i=0}^{n−1} i = n(n − 1)/2`.
- If it depends on an index with non-unit, geometric or square-root stepping: `unknown` ("inner loop bounds depend
  on an index whose iteration space is not a unit-step range").

If **any** loop inside a nest has no expression (unknown or unsupported), the whole nest has no expression and takes
the weakest status: one binary-search `while (lo < hi)` inside a `while (t--)` wrapper makes the entire
test-case nest `unknown`. Whole-nest results are never assembled from partially known parts.

Pattern precedence for a nest: `test-case-loop` (outermost loop is `while (t--)`; applies even when the nest has no expression), then `unsupported-loop` /
`data-dependent-loop` (no expression), `triangular-loop`, `nested-loop` (depth > 1), `logarithmic-loop`,
`sqrt-loop`, `range-for-loop`, `linear-loop`.

### 2.2 Recorded assumptions

| Situation | Assumption text (verbatim prefix) |
| --- | --- |
| Increasing loop | `loop bound assumed to be at least the start value` |
| Decreasing loop | `loop start assumed to be at least the bound` |
| Geometric growth / decay | `geometric index growth: at most log_c(bound) + 1 iterations` / `geometric index decay: at most log_c(start) + 1 iterations` |
| Square-root loop | `index starts at a non-negative value; at most sqrt(bound) + 1 iterations` |
| Test-case loop | `'t' counts iterations (assumed non-negative at loop entry)` |
| Countdown while | `'x' holds its value at loop entry and is non-negative` |
| Range-for | `the container is not resized during iteration` |
| Early exit | `an early exit (break/return/goto/throw) may end a loop sooner; the count is an upper bound` |

Trip counts are not clamped at zero: the "bound at least start" assumption makes a negative count impossible under
the assumption; under constraints the workload evaluator treats the expression as written.

---

## 3. Status rules

Applied per loop and combined with "weakest wins" (proven > estimated > unknown > unsupported).

| Condition | Bound status | Recorded as |
| --- | --- | --- |
| Supported form, no issues | `proven` | — |
| Loop index written in the body | `unsupported` | limitation `loop index 'i' is modified in the body` |
| `while (t--)` counter written in the body | `unsupported` | limitation `counter 't' is modified in the body` |
| Non-integer index | `unsupported` | `only integer loop indices are modelled` |
| Unsigned index compared `>= 0` | `unsupported` | `unsigned index compared with >= 0 never terminates` |
| `do … while` | `unsupported` | `do-while loops are not modelled yet` |
| Bound variable possibly written in the body | `estimated` | limitation `bound variable 'n' may be modified inside the loop; the count assumes its initial value` |
| Call to a user-defined function (defined in the main file) in the nest, excluding the implicit `begin()`/`end()` calls of a range-for header | `estimated` | limitation `calls user function 'f'; its cost is not included (interprocedural analysis is not implemented)` |
| Library calls in the nest (excluding operator calls, constructors and the implicit `begin()`/`end()` calls of a range-for header) | unchanged | limitation `per-iteration cost of library calls is not included: …` (up to 6 names) |
| Early exit (`break`, `return`, `goto`, `throw`) | unchanged (still an upper bound) | assumption, see §2.2 |
| Non-canonical `for`, unsupported bound expression, other `while` shapes (e.g. `while (lo < hi)` binary search, Collatz) | `unknown` | `data-dependent termination: the iteration count depends on values computed in the loop` or a more specific reason |

A finding without an expression always has status `unknown` or `unsupported`.

**Confidence** for loop and recursion findings (derived, never an input):

| Bound status | Condition | Confidence |
| --- | --- | --- |
| proven | `limitations` empty and no early exit | `high` |
| proven | otherwise | `medium` |
| estimated | — | `low` |
| unknown, unsupported | — | `none` |

Storage findings: `static-array` is `proven` / `high`; `sized-vector` is `estimated` / `medium`.

---

## 4. Patterns

| Pattern | Category | Produced when |
| --- | --- | --- |
| `linear-loop` | `loop_nest` | Single counted loop with linear trip count |
| `nested-loop` | `loop_nest` | Depth > 1, no index-dependent inner bound |
| `triangular-loop` | `loop_nest` | Inner bound depends on an outer index (summation) |
| `logarithmic-loop` | `loop_nest` | Geometric for-loop or dividing while-loop |
| `sqrt-loop` | `loop_nest` | `i * i <= n` loop |
| `range-for-loop` | `loop_nest` | Range-based for |
| `test-case-loop` | `loop_nest` | Outermost loop is `while (t--)` / `while (t-- > 0)` |
| `data-dependent-loop` | `loop_nest` | No expression, status `unknown` |
| `unsupported-loop` | `loop_nest` | No expression, status `unsupported` |
| `direct-recursion` | `recursion` | Function calls itself at one call site |
| `multiple-direct-recursion` | `recursion` | Function calls itself at more than one call site |
| `static-array` | `static_storage` | Global/static array (exact bytes, `proven`) |
| `sized-vector` | `dynamic_allocation` | `std::vector<T> v(count, …)` with symbolic `count` (`count × sizeof(T)`, `estimated`) |

Finding ids are `F1`, `F2`, … in source order. Providers: see
[ANALYSIS_MODEL.md §7](../design/ANALYSIS_MODEL.md#7-provenance).

---

## 5. Asymptotic derivation

`asymptoticUpperBound(expr)` returns a sum of mutually non-dominated monomials `Π v^a (log v)^b`, assuming every
variable grows without bound and is ≥ 1. Rendered as e.g. `O(n^2)`, `O(m n)`, `O(n log n)`, `O(m n + n^2)`,
`O(log n)`, `O(1)`.

| Node | Rule |
| --- | --- |
| Constant | `1` (empty monomial) |
| Variable `v` | `v` |
| Add, Maximum | Union of operand terms, then remove dominated terms. Add operands that are non-positive constants or products led by a negative constant are **dropped** (keeps an upper bound) |
| Multiply | Product of factor terms; constant factors ignored |
| Power `a^k` | Each term of `a` raised to `k` (multinomial expansion bounded by dominant terms) |
| Logarithm | `log(Σ monomials)` → `Σ log v` over variables with positive power; base ignored; `log log v` unsupported → `null` |
| Summation `Σ_{i=lo}^{hi} body` | `hi × body[i := hi]` (count ≤ upper bound, each term ≤ body at the upper limit) |

The result is an **upper bound (O)**, not a tight bound (Θ). Example: the triangular count `n(n − 1)/2` derives to
`O(n^2)`. When derivation is not possible the protocol field `asymptotic` is `null`; the expression remains valid.

---

## 6. Evaluator semantics

`evaluate(expr, bindings, options)`:

| Aspect | Behaviour |
| --- | --- |
| Arithmetic | IEEE-754 double. Results above 2⁵³ are approximate |
| Non-finite intermediate | Status `Overflow` ("value exceeds the representable range"); never returns `inf`/`NaN` |
| Unbound variable | Status `UnboundVariable` |
| Fractional power of a negative value | Status `DomainError` |
| Logarithm | `log_base(max(x, 1))` |
| Summation range | `[⌈lo⌉, ⌊hi⌋]`, empty → 0 |
| Summation with body affine in the index | Closed form `count · c0 + c1 · (lo + hi) · count / 2` |
| Other summations | Term by term; each node evaluation consumes budget |
| Budget | `EvalOptions::budget`, default 5·10⁷ node evaluations; exceeded → `BudgetExceeded` |

Workload evaluation maps any non-`Ok` status to workload `unknown` with an assumption "evaluation failed: …".

---

## 7. Storage analysis

| Construct | Item | Size |
| --- | --- | --- |
| Global or static variable | `global_static` | Exact bytes from Clang type layout; summed with saturation at `UINT64_MAX` (`static_storage_saturated`) |
| Constant-size local array | `local_array` | Exact bytes; only the largest is reported |
| `std::vector<T> v(count, …)` with non-constant `count` | `vector_buffer` | Symbolic `count × sizeof(T)` (e.g. `4 * n`) |

Assumptions on sized vectors: "element buffer only; std::vector bookkeeping and allocator overhead excluded";
limitation: "elements that own further heap memory (e.g. nested containers) are not followed".

---

## 8. STL cost model (data only)

`engine/src/estimation/stl_cost_model.cpp` holds a table of average/worst/amortized costs (vector `operator[]`,
`push_back` amortized, `erase`; map `find`/`operator[]`; set `insert`/`find`; unordered_map `find`/`operator[]`
average O(1) worst O(n); priority_queue `push`/`pop`/`top`; `sort` O(n log n); `lower_bound`). It is exposed through
the `capabilities` action (`stl_cost_model: data_only`) and is **not applied to findings** yet (planned, M2/M3
follow-up). Until then, library calls appear only as limitations.

---

## 9. Recursion

**Implemented (BL-202): detection and recurrence extraction.** Direct self-calls are found by canonical
declaration identity; mutual recursion is not detected. Each recursive function yields one `recursion` finding:
pattern `direct-recursion` or `multiple-direct-recursion`, provenance `clang-recursion-detector`. The front end
extracts a front-end-independent `complexity::Recurrence`
(`engine/include/bigolens/complexity/recurrence.hpp`):

| Part | How it is determined |
| --- | --- |
| Size measure `n` | The first integer parameter `p` whose argument at every self-call is linear in `p` alone and shrinks: `p − c` (c ≥ 1, **Decrement**) or `β·p + γ` with `β = 1/b`, `b ≥ 2` an integer, `γ ≤ 1` (**Divide**). Otherwise a pair `(lo, hi)` of integer parameters whose new difference `hi' − lo'` is linear, translation invariant, and shrinks in the same way, giving `n = hi − lo`. Arguments may go through locals that are never written (`const int mid = lo + (hi − lo) / 2`). Linear forms are determined exactly from 2–3 evaluations; integer division is treated as real division, which is recorded as an assumption. |
| Calls | One `RecursiveCall` per self-call site, with its change (Decrement / Divide / Unknown), the arguments as written, and whether it is inside a loop. |
| Calls per invocation | The largest number of self-calls on any single execution path. `if`/`else`, `?:` and `return` are path-sensitive, so binary search has two call sites but at most one call per invocation. A self-call inside a loop makes it unknown. |
| Work `f(n)` | The sum of the function's loop-nest costs (1 if there are none), rewritten in terms of `n`. For intervals, `lo → 0, hi → n`, checked to depend only on `hi − lo`. Unknown if a loop has an unknown count or contains a self-call. Calls to other user functions are listed as not included. |

Without a solver the finding stays `unsupported` / confidence `none` / `time: null`. Its limitations start with the
recurrence and the reason, for example:

```text
extracted recurrence: T(n) = T(n - 1) + T(n - 2) + O(1)
recurrence solving is not implemented yet; no bound is reported
```

Other renderings: `T(n) = 2 T(n/2) + O(n), where n = hi - lo` (merge sort); `T(n) = at most 1 of {T(n/2),
T(n/2)} + O(1), where n = hi - lo` (binary search); `T(m) = T(m - 1) + O(1), where m = limit - n` (counting up;
the symbol avoids clashing with a source identifier `n`). Shapes that are not recognised render as `T(?)`, with "no
parameter that shrinks at every call".

**Solving (planned, BL-203).** `complexity::RecurrenceSolver` implementations, passed to the `RequestHandler`,
are tried in order by `analyzers::resolveRecurrences`. A solution sets the time expression, status and confidence,
and moves the recurrence into `assumptions`. Planned solvers: the master theorem for `a·T(n/b) + f(n)` and
summation for `T(n − c) + f(n)`. Fibonacci-like multiple subtractive recursion stays unsupported. The capability
`recurrence_extraction` is `partial`; `recursion_analysis` stays `unsupported` until a solver exists.
