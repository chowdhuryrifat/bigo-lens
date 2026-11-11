# Branch Protection Rules and PR Policy

This document defines the branch protection configuration and pull request policy for BigO Lens. These rules protect the integrity, honesty, and releasability of the `main` branch.

---

## 1. Protected Branch: `main`

The `main` branch is the production trunk. It is protected by GitHub branch rulesets. Direct pushes and force pushes to `main` are strictly forbidden.

### 1.1 Pull Request Requirements
- **Require a pull request before merging**: Every change must arrive via a pull request.
- **Approvals**: At least **1 approving review** from a code owner or core maintainer is required.
- **Dismiss stale reviews**: Approvals are automatically dismissed when new commits are pushed to the PR branch.
- **Reviewer restrictions**: Authors may not approve their own pull requests.
- **Conversation resolution**: All review comments and conversations must be resolved before merging.

### 1.2 Required Status Checks (Passing CI)
The following automated status checks must pass before merge is enabled:
- `Rust · Workspace checks & tests` (`cargo test --workspace`)
- `Rust · Format & Clippy` (`cargo fmt --check`, `cargo clippy -- -D warnings`)
- `Native · Ubuntu · Clang frontend` (Linux C++ Clang build & CTest)
- `Native · windows-2022 · core` (Windows C++ core build & CTest)
- `Native · macos-latest · core` (macOS C++ core build & CTest)
- `Extension · ubuntu-24.04` (TypeScript compilation, unit & integration tests)
- `PR Branch & Conventions Check` (Branch prefix and conventional commit title validation)
- `clang-format` & `clang-tidy`
- `typescript` (ESLint and Prettier checks)
- **Require branches to be up to date**: Branches must be rebased onto latest `main` before merge.

### 1.3 Linear History and Merge Strategy
- **Squash and merge only**: Merge commits and rebase-merges are disabled.
- **Commit message format**: The squash commit title and description must follow [Conventional Commits](https://www.conventionalcommits.org/): `<type>(<scope>): <summary>`.
- **Force push restriction**: Force pushes (`git push --force`) to `main` are blocked for all users, including administrators.
- **Branch deletion restriction**: Deletion of `main` is blocked.

---

## 2. PR Branching Conventions

Contributors and agents must branch from `main` using one of the permitted prefixes:

| Branch Prefix | Allowed Purpose | Example |
|---|---|---|
| `feat/` | New user-facing capability or analyzer feature | `feat/bl-304-nested-vectors` |
| `fix/` | Bug fix or soundness correction | `fix/triangular-loop-bound` |
| `docs/` | Documentation, specifications, or ADRs | `docs/adr-0013-multi-language` |
| `chore/` | Tooling, dependencies, maintenance | `chore/update-dependencies` |
| `refactor/` | Behavior-preserving internal reorganization | `refactor/rust-ast-traits` |
| `test/` | Test suites, fixtures, or benchmarks | `test/vector-allocation-fixtures` |
| `ci/` | CI workflows and automation | `ci/add-rust-clippy-job` |
| `perf/` | Measured performance optimization | `perf/fast-asymptotic-reduction` |

Branches not matching this pattern (or `dependabot/*`) are rejected by the `PR Branch & Conventions Check` CI workflow.

---

## 3. GitHub Ruleset Configuration (`main`)

Repository administrators configure the ruleset under **Settings -> Rules -> Rulesets -> New ruleset**:
- **Ruleset Name**: `Protect main`
- **Enforcement status**: `Active`
- **Target branches**: `Include default branch`
- **Rules enabled**:
  - Restrict deletions: Enabled
  - Block force pushes: Enabled
  - Require a pull request before merging:
    - Required approvals: `1`
    - Dismiss stale pull request approvals when new commits are pushed: `true`
    - Require review from Code Owners: `true`
    - Require approval of the most recent reviewable push: `true`
  - Require status checks to pass:
    - Require branches to be up to date before merging: `true`
    - Status checks: listed above in §1.2
  - Require linear history: `true`
