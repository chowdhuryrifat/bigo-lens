# BigO Lens — Product Design

> Status: version 0.1.0, protocol 1.0. Milestone **M1 Foundation** is in progress. Parts of **M2 Static analysis**
> and **M3 Constraints & memory** exist as early, partial implementations. Anything not marked *implemented* in
> this document is planned and is labelled with its milestone.

Related documents: [ARCHITECTURE.md](ARCHITECTURE.md) · [RULES.md](RULES.md) · [SECURITY.md](SECURITY.md) ·
[Analysis model](docs/design/ANALYSIS_MODEL.md) · [UI/UX design](docs/design/UI_UX_DESIGN.md) ·
[Requirements](docs/specifications/REQUIREMENTS.md) · [Limitations](docs/specifications/LIMITATIONS.md) ·
[Roadmap](docs/development/ROADMAP.md)

---

## 1. Vision

BigO Lens tells a competitive programmer, before they submit, **how much work their C++ solution does under the
problem's actual constraints**, and **how sure that answer is**. It does this locally, offline, inside the editor,
without depending on any judge or contest platform and without requiring a language model.

The product's core promise is honesty: a precise answer when one can be proven, a clearly labelled estimate when it
cannot, and an explicit "unknown" or "unsupported" instead of a guess.

## 2. Problem statement

Competitive programmers routinely judge feasibility by hand: "N ≤ 2·10⁵, my loop is O(N²), that is 4·10¹⁰, too
slow." This mental arithmetic fails in predictable ways:

1. **Constraint relationships are ignored.** With `T ≤ 10`, `N ≤ 2·10⁵` and `ΣN ≤ 2·10⁵`, a per-test-case O(N²)
   loop costs at most 4·10¹⁰ elementary iterations, not T·N² = 4·10¹¹. Mistakes in both directions are common:
   rejecting a correct solution, or accepting one whose real worst case is larger than assumed.
2. **Complexity is conflated with runtime.** O(N log N) says nothing about whether 2 seconds is enough; constant
   factors, memory access patterns and the machine decide that.
3. **Structure is misread.** Triangular loops, halving loops, `i * i <= n` loops and `while (t--)` wrappers are easy
   to over- or under-count.
4. **Memory is guessed.** Static arrays and `std::vector<T>(n)` buffers add up; a solution can fail on memory with
   a perfectly fine time complexity.
5. **Existing tools are either generic profilers** (they need inputs and give numbers without asymptotic context) or
   **opaque estimators** that state a complexity with no evidence, provenance or confidence.

## 3. Target users

| User | Need | What BigO Lens provides |
| --- | --- | --- |
| Competitive programmer (practice and contests) | Fast pre-submission sanity check | Per-loop-nest complexity, constraint-aware workload, memory lower bounds |
| Learner studying algorithms | Understand *why* a loop is O(n log n) | Expression, trip-count assumptions, explanation of each finding |
| Problem setter / tester | Check that intended and naive solutions separate under the chosen limits | Workload numbers under exact constraints (benchmarking in M4) |
| Coach / instructor | Explain complexity with concrete numbers | Reproducible, offline output; JSON protocol for scripting |

Out of scope as primary users: production performance engineers (see [non-goals](#9-non-goals)).

## 4. Core use cases

| ID | Use case | Status |
| --- | --- | --- |
| UC-1 | Analyze the active C++ file and list loop nests with their structural count expression and O() bound | Implemented (partial, M2) |
| UC-2 | Enter problem constraints (variable bounds, ΣN-type sum constraints, time and memory limits) for a file | Implemented via sidecar JSON file (M3 partial); form UI planned (M5) |
| UC-3 | See the worst-case workload of each finding under the constraints, with the evaluation method | Implemented (partial, M3) |
| UC-4 | See static memory lower bounds (globals, static arrays, sized vectors) | Implemented (partial, M3) |
| UC-5 | See feasibility against time and memory limits | Memory: implemented (HighRisk only from lower bounds). Time: always *Unknown* until M4 |
| UC-6 | Navigate from a finding to its source location | Implemented (tree view click, Problems panel) |
| UC-7 | Understand limitations of a finding (unsupported loop, data-dependent loop, recursion, library calls) | Implemented (assumptions and limitations lists) |
| UC-8 | Benchmark the solution on generated inputs and obtain a runtime range | Planned (M4) |
| UC-9 | Paste a problem statement and get *candidate* constraints to confirm | Planned (post-M5) |
| UC-10 | Analyze recursive functions via recurrence solving | Planned (M2+ follow-up) |

## 5. User workflows

### 5.1 Implemented today (M1 scaffold)

1. Open a `.cpp` file in VS Code.
2. Run **BigO Lens: Edit Constraints for Active File**. The extension creates `<file>.bigolens.json` from a template
   (if absent) and opens it. The user fills in the `constraints` object defined by the
   [request schema](shared/schemas/request.schema.json).
3. Run **BigO Lens: Analyze Active C++ File**. The extension validates the sidecar file, spawns `bigolens-engine`,
   sends one JSON request on stdin and validates the JSON response.
4. The **Analysis Results** tree view (activity bar container *BigO Lens*) lists findings with pattern, O() bound,
   workload, bound status and confidence, plus memory and feasibility summaries. Information-level diagnostics
   appear in the Problems panel. Clicking a finding navigates to its source range.
5. **BigO Lens: Show Engine Capabilities** shows which analyses the installed engine supports.

### 5.2 Planned

- **M5:** constraint form UI, webview report with explanations, status-bar summary, re-analysis on save (opt-in).
- **M4:** user-initiated benchmark run with progressive input scaling and a runtime range.
- **Later:** problem-statement extraction producing candidate constraints that the user must confirm.

See [UI_UX_DESIGN.md](docs/design/UI_UX_DESIGN.md) for wireframes and the user-flow diagram.

## 6. Product principles

These are binding; [RULES.md](RULES.md) and [ENGINEERING_RULES.md](docs/engineering/ENGINEERING_RULES.md) restate them
as engineering rules.

1. **Correctness over appearance.** An empty or "unknown" result is better than a plausible wrong one. No result is
   ever padded, rounded towards a nicer answer, or fabricated.
2. **Keep the quantities separate.** Asymptotic complexity, structural operation count, runtime and memory are
   different things, are computed by different mechanisms, and are always reported under different labels. See the
   distinction table in [ANALYSIS_MODEL.md](docs/design/ANALYSIS_MODEL.md#2-quantities-and-what-they-are-not).
3. **Bound status is not confidence.** Every quantity carries a *bound status* (`proven`, `estimated`, `unknown`,
   `unsupported`) describing what kind of claim it is, and separately a *confidence* (`high`, `medium`, `low`,
   `none`) describing how much the user should trust it given assumptions and limitations.
4. **Constraint relationships are first-class.** Sum constraints such as ΣN ≤ 2·10⁵ must be used, never silently
   dropped. Relationships that are accepted but not yet exploited (product, linear) produce an explicit assumption
   saying so.
5. **Provenance on every finding.** Each finding names the analyzer that produced it and lists its assumptions and
   limitations.
6. **Platform independence.** No dependency on, scraping of, or integration with any competitive programming
   platform or online judge. Constraints come from the user.
7. **Offline and deterministic by default.** All analysis runs locally. No network access is needed.
8. **No mandatory LLM.** Any future language-model assistance is optional, off by default, and never the source of a
   proven claim.
9. **Never execute user code implicitly.** Static analysis parses only. Execution (M4 benchmarking) is always
   user-initiated and resource-limited; until real isolation exists it is documented as not sandboxed.

## 7. Feature boundaries

| Area | Inside the boundary | Outside the boundary |
| --- | --- | --- |
| Language | C++ (C++11–C++23), single translation unit | Other languages; multi-file projects |
| Static analysis | Counted loops, countdown `while` loops, range-for, direct recursion detection, storage | Arbitrary termination proofs, interprocedural cost (planned), pointer/alias analysis |
| Constraints | Variable bounds, sum, product, linear relations, time and memory limits | Distributional assumptions ("random input"), adversarial input construction |
| Runtime | Planned (M4): measured on the user's machine, scaled to an estimate with an explicit range | Predicting a specific judge machine's time exactly |
| Memory | Static lower bounds, sized `std::vector` buffers; measured peak RSS planned (M4) | Exact peak heap usage from static analysis |
| Output | VS Code UI, JSON protocol | Online dashboards, telemetry |

## 8. MVP scope

The MVP is the completion of milestones **M1–M3 plus M5**, with M4 benchmarking as the first post-MVP feature if it
is not ready. MVP acceptance requires:

- All loop forms listed in [COMPLEXITY_MODEL.md](docs/specifications/COMPLEXITY_MODEL.md#2-supported-loop-forms)
  analysed with correct status rules, verified by fixtures in `tests/fixtures/cpp/`.
- Constraint-aware workload with sum-constraint packing, verified by the T/N/ΣN example.
- Memory lower bounds and memory HighRisk detection.
- VS Code extension with commands, tree view, diagnostics, source navigation and the constraint sidecar file;
  constraint form UI (M5).
- Linux as primary platform (full build including the Clang frontend). macOS and Windows: core engine supported,
  Clang frontend experimental (see [LIMITATIONS.md](docs/specifications/LIMITATIONS.md#15-platform-support)).
- Release artifacts (engine for Linux, `.vsix`) produced by the release workflow (M6).

Current progress against these items is tracked in [REQUIREMENTS.md](docs/specifications/REQUIREMENTS.md) and
[ROADMAP.md](docs/development/ROADMAP.md).

## 9. Non-goals

- **Proving complexity of arbitrary programs.** Impossible in general (halting problem); BigO Lens reports
  `unknown` instead.
- **Replacing a profiler.** BigO Lens does not attribute runtime to lines or functions.
- **Judging or submitting solutions.** No interaction with online judges.
- **Correctness checking.** BigO Lens does not test whether the solution is right.
- **Cloud analysis, accounts or telemetry.**
- **Mandatory AI assistance.**
- **Exact runtime prediction on unknown hardware.** Runtime estimates (M4) are ranges with stated assumptions.

## 10. Future development

| Item | Milestone | Notes |
| --- | --- | --- |
| Recurrence analysis for direct recursion (master theorem, linear recurrences) | M2 follow-up | Interface sketched in [COMPLEXITY_MODEL.md](docs/specifications/COMPLEXITY_MODEL.md#9-recursion) |
| Interprocedural cost (callee cost substitution) | M2 follow-up | Today callee cost is excluded and the finding is downgraded to `estimated` |
| Applying the STL cost model to findings | M2/M3 follow-up | Table exists and is exposed via `capabilities`; not applied |
| Product and linear constraints used to tighten bounds | M3 follow-up | Parsed and validated today |
| Benchmarking: compile, generate inputs, measure, fit growth, runtime range | M4 | Statistics and growth fitting library exist |
| Time feasibility from a runtime range | M4 | Policy implemented; input missing |
| Constraint form UI, webview report, explanations | M5 | |
| Problem-statement constraint extraction (offline heuristics; optional LLM) | After M5 | Candidates always require user confirmation |
| Validation corpus and packaging, Marketplace publication decision | M6 | Release workflow builds `.vsix` today; no Marketplace publishing |
