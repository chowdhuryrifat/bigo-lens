# Frequently Asked Questions

## General

### What does BigO Lens do?

It parses your C++ file with Clang, finds loop nests, recursion and storage, and reports for each:

- a **structural count** expression: how many times the innermost loop bodies execute
- its **asymptotic class**, such as `O(n log n)`
- the **worst-case workload** under the constraints you enter, using relationships such as Σn ≤ 2·10^5
- **static memory** lower bounds and estimates
- a **feasibility** verdict against your time and memory limits

Every result carries a bound status, a confidence level, assumptions and limitations.

### Is it on the VS Code Marketplace?

Not yet. Build it from source and run it in the Extension Development Host (see [GETTING_STARTED.md](GETTING_STARTED.md)). No Marketplace publication has happened, and none is promised yet. See [ROADMAP.md, M6](../development/ROADMAP.md#m6-validation-and-release).

### Does it work with online judges or contest platforms?

BigO Lens does not connect to, scrape or integrate with any platform. You enter the constraints yourself. Everything runs locally and offline.

### Does it use AI or send my code anywhere?

No. Analysis is local and deterministic. No network access is needed, and there is no telemetry.

## Understanding results

### Why is my loop reported as `unknown`?

The analyzer only claims a bound for loop shapes it can justify (see [COMPLEXITY_MODEL.md](../specifications/COMPLEXITY_MODEL.md)). Loops whose termination depends on runtime data are reported as `unknown`, with "data-dependent termination". Examples are binary search's `while (lo < hi)`, the Collatz iteration, and two-pointer loops (which are not supported yet). This is deliberate: a guessed `O(log n)` that is sometimes wrong would be worse than an honest `unknown`.

### Why is a finding `unsupported`?

The construct is recognized but not analysed. Examples are a loop whose index is modified inside the body, `do-while` loops, and recursion. Recursion is detected, but recurrence solving is not implemented yet.

### What is the difference between bound status and confidence?

**Bound status** says what kind of claim it is:

- `proven`: derived exactly, under the stated assumptions
- `estimated`: a heuristic step was involved, for example a bound variable that may be modified in the body, or a call to one of your own functions
- `unknown`: no bound could be determined
- `unsupported`: the construct is not handled

**Confidence** (`high`, `medium`, `low`, `none`) says how much to rely on it, given its assumptions and limitations. For example, a `proven` loop with an early `break` is still a valid upper bound, but has `medium` confidence, because the bound may be far from the typical case.

### Is the "workload" the number of operations my program executes?

No. It is the **structural count** of loop-body executions at the worst case your constraints allow. A body counts as 1 no matter how much work it does, unless it contains loops of its own. It is not a CPU instruction count and not a runtime. Use it to compare against the usual rule of thumb for your judge, keeping in mind what it leaves out.

### Why doesn't the workload include the cost of `std::sort`, `map::find`, or my helper function?

- **Library calls:** a cost table for common STL operations exists, and you can see it with **Show Engine Capabilities**, but it is **not yet applied** to findings. Such calls are listed in the finding's `limitations`.
- **Your own functions:** interprocedural analysis is not implemented. A loop that calls a user-defined function is marked `estimated`, and the callee's cost is **not** included.

Both are planned (see [ROADMAP.md, M2](../development/ROADMAP.md#m2-static-analysis)).

### Why is the workload 4·10^10 and not 4·10^11 for my multi-test problem?

With t ≤ 10, n ≤ 2·10^5 and Σn ≤ 2·10^5, all ten tests cannot be at maximum size at once. For an O(n²) per-test cost, the worst case packs the whole budget into one test: (2·10^5)² = 4·10^10. BigO Lens uses this **sum-constraint packing** when the per-test cost is recognized as superadditive, and otherwise falls back to a looser but sound bound. You must declare the sum constraint in the constraints file for this to happen.

### Why does time feasibility always say `unknown`?

Deciding whether a solution fits a time limit requires a runtime estimate, and that requires measurement on your machine. Benchmarking is **not implemented yet** (milestone M4). BigO Lens does not convert operation counts into seconds by guessing a speed.

### Why does memory feasibility say `unknown` even though my arrays are small?

Static analysis provides **lower bounds** (globals and static arrays, computed exactly) and **estimates** (sized `std::vector` buffers). A verdict of "likely safe" requires an *upper* bound on memory use, which static analysis cannot provide. BigO Lens therefore reports `high_risk` when even the lower bound exceeds the limit, and `unknown` otherwise.

### My constraint isn't being applied. Why?

- The name must match the identifier **in your code** (see [CONFIGURATION.md §2.2](CONFIGURATION.md#22-format)).
- Product and linear constraints are accepted, but **not yet used** to tighten bounds. An assumption in the result says so.
- Check the **BigO Lens** output channel for validation errors in the constraints file.

## Safety

### Does BigO Lens run my code?

No. The engine only **parses** your code (Clang `-fsyntax-only`). It never compiles to a binary, and never executes anything. Like any compiler, it reads the headers your file includes. Benchmarking, which would run your code, is planned. It will be user-initiated only, and documented as not sandboxed until real isolation exists. See [SECURITY.md](../../SECURITY.md).

### Is it safe to analyse code I copied from elsewhere?

Analysing untrusted code carries the same exposure as running a compiler's syntax check on it: Clang reads whatever headers the file includes from your file system. Results are shown only to you, and nothing is executed. See [THREAT_MODEL.md](../engineering/THREAT_MODEL.md).

## Platforms and setup

### Which platforms are supported?

Linux is the primary platform, with full analysis. On macOS and Windows, the core engine builds and is tested, but the Clang front end that performs source analysis is **experimental**. See [TROUBLESHOOTING.md §9](../development/TROUBLESHOOTING.md#9-platform-notes-macos-and-windows).

### Which C++ standards are supported?

The source is parsed as C++11, 14, 17, 20 or 23, using the `bigoLens.cppStandard` setting (default `c++20`). Analysis covers one translation unit, the file you are editing.

### I get `frontend_unavailable`. What does that mean?

Your engine was built without the Clang front end (the `no-clang` preset). Rebuild with the `dev` or `release` preset. See [TROUBLESHOOTING.md §6](../development/TROUBLESHOOTING.md#6-frontend_unavailable-error).

### Where do I report a bug or request a feature?

Open a GitHub issue using the **bug report**, **feature request** or **documentation** template. For security issues, use private vulnerability reporting instead (see [SECURITY.md](../../SECURITY.md)).
