# ADR 0005: Represent costs as immutable symbolic expression trees

- **Status:** Accepted
- **Date:** 2026-11-10

## Context

Producing a plain string such as `"O(n^2)"` at analysis time throws information away. That information is needed to:

- Evaluate a concrete worst-case workload under user constraints (`t <= 10`, `n <= 2e5`).
- Exploit relationships such as sum constraints, where `t * n^2` with a sum of n at most 2e5 gives 4e10 rather than 4e11.
- Keep exact structural counts, for example a triangular loop is `sum_{i=0}^{n-1} i`, which is n(n-1)/2, not "n squared".
- Derive the O() form separately, under explicit assumptions.

## Decision

Implement `bigolens::complexity` as an **immutable, shared expression tree**:

- `ExprKind { Constant, Variable, Add, Multiply, Power, Logarithm, Summation, Maximum }`. `Power` takes a constant exponent of at least 0. `Logarithm` takes a base above 1 and evaluates the log of max(x, 1).
- `using ExprPtr = std::shared_ptr<const Expr>`. Nodes are never mutated, so subtrees are shared safely.
- Factories (`constant`, `variable`, `add`, `subtract`, `multiply`, `power`, `logarithm`, `summation`, `maximum`) apply **only semantics-preserving simplification**: flattening, constant folding and identity removal. Invalid construction throws `std::invalid_argument`.
- Three separate consumers:
  - `toString`: canonical human-readable rendering.
  - `evaluate`: IEEE double evaluation. Non-finite results give `Overflow`. Unbound variables are reported. Non-affine summations are bounded by a budget of 5e7 node evaluations.
  - `asymptoticUpperBound`: non-dominated monomials n^a (log n)^b, assuming variables grow without bound and are at least 1. Negative terms are dropped and unsupported shapes return null.

## Alternatives considered

| Alternative | Why it was rejected |
|---|---|
| Strings or enums for complexity classes | Cannot be evaluated or combined with constraints. |
| External computer-algebra system (SymPy, GiNaC) | A heavy runtime dependency, and aggressive rewriting that can obscure how a result was derived. |
| Mutable AST with in-place simplification | Aliasing bugs, and harder to share between findings. |
| Closed-form-only representation | Many sums have no simple closed form. Keeping `Summation` preserves exactness. |

## Consequences

**Positive**

- One representation supports display, numeric workloads, O() derivation and constraint reasoning (`isMonotoneNonDecreasing`, `isSuperadditiveIn`).
- Immutability makes sharing and reasoning simple.
- Conservative simplification means a rendered expression matches what was counted.

**Negative**

- Rendered forms can be verbose, for example `t * n * n` rather than `t * n^2`.
- Double precision. Values above 2^53 are approximate.
- Each new construct, such as recurrences, needs support in every consumer.

## Follow-up considerations

- Recurrence nodes or a separate recurrence-solving interface for recursion (planned; recursion is `Unsupported` today).
- Optional display normalisation (collecting equal factors into powers) as a rendering-only step.
- Interval or exact integer evaluation for very large bounds.
