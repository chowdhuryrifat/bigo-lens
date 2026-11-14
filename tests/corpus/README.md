# BigO Lens Validation Corpus

This directory contains the validation corpus of representative algorithmic solutions used for empirical evaluation and regression scoring of the BigO Lens static analysis and asymptotic complexity engine (BL-601).

## License

All programs in this corpus are original works authored specifically for the BigO Lens project and are released under the project's [Apache-2.0 License](../../LICENSE) (or MIT where applicable). They do not incorporate copyrighted content or code scraped from competitive programming platforms or online judges.

## Structure

- `manifest.json`: The machine-readable catalog of validation benchmark cases, specifying the source file, constraints, expected dominant asymptotic complexity class, expected bound status, expected pattern, and explanatory rationale.
- `src/`: The self-contained C++ solutions representing standard algorithmic structures and patterns:
  - Linear scans, prefix sums, and two-pointer loops
  - Nested loops, triangular forms, and grid traversals
  - Logarithmic patterns and halving loops
  - Divide-and-conquer recurrences and Master Theorem forms
  - Standard template library (STL) algorithms and containers
  - Memory allocations (arrays, vectors, nested 2D vectors)
  - Deliberate near-miss and unanalyzable patterns (Collatz loops, data-dependent termination, multiple subtractive recursion) to verify adherence to the Prime Directive: **The engine never guesses; unprovable cases must report `unknown` or `unsupported`.**

## Scoring Harness

The corpus is evaluated using `scripts/test/score-corpus.py`. The scoring harness:
1. Runs the BigO Lens engine over every solution in the manifest.
2. Validates the engine output against `response.schema.json`.
3. Compares the derived dominant asymptotic complexity and bound status against ground truth.
4. Generates a reproducible report containing overall precision, per-pattern accuracy, and unknown/unsupported percentages.
