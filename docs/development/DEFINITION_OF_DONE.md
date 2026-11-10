# Definition of Done

A change is **done** only when every applicable item below is true. Reviewers use this list, and issues in [IMPLEMENTATION_PLAN.md](IMPLEMENTATION_PLAN.md) add their own items on top of it.

Mark an item as "N/A" in the pull request description only when it genuinely does not apply, for example "Protocol: N/A, no protocol change".

## 1. Scope and correctness

- [ ] The acceptance criteria of the linked issue are met. Any criterion left out is moved to a new issue that the pull request links.
- [ ] Results stay honest. No path reports a stronger `bound_status` or `confidence` than the analysis justifies. Unrecognized shapes produce `unknown` or `unsupported` with a limitation, never a guess.
- [ ] Assumptions and limitations are filled in for any new or changed finding, workload method or feasibility rule.
- [ ] `capabilities.features` reflects any change in feature maturity.

## 2. Code quality

- [ ] The code follows [RULES.md](../../RULES.md) and [CODING_STANDARDS.md](../engineering/CODING_STANDARDS.md), and none of the [prohibited practices](../../RULES.md#16-prohibited-practices) appear.
- [ ] It builds without warnings under `BIGOLENS_WARNINGS_AS_ERRORS=ON` (the `ci` preset).
- [ ] The `no-clang` preset still builds and passes, because `bigolens_core` gained no Clang dependency.
- [ ] clang-format, clang-tidy, ESLint and Prettier checks pass (`scripts/development/format.sh` has been run).
- [ ] No dead code, no commented-out code, and no `TODO` without an issue reference (`TODO(BL-123)`).

## 3. Tests

- [ ] New behaviour has tests at the right layer (see [TESTING_STRATEGY.md](../engineering/TESTING_STRATEGY.md)).
- [ ] A bug fix includes a regression test that fails without the fix.
- [ ] A new analysis pattern has a positive fixture and a near-miss fixture in `tests/fixtures/cpp/`.
- [ ] A new validation rule or error code has a negative test.
- [ ] Tests are deterministic: no timing thresholds, no network, no machine-specific paths.
- [ ] The author ran the relevant suites locally and lists the exact commands and outcomes in the pull request:
  - `ctest --preset dev` (or `no-clang` where Clang is unavailable, saying so)
  - `npm test`, plus `npm run test:integration` when the engine and extension interact
- [ ] CI is green. Only the experimental macOS Clang job may fail.

## 4. Protocol and compatibility

- [ ] Protocol changes update, in the same pull request: `shared/schemas/*.json`, the C++ validator and serializer, the TypeScript types, [JSON_PROTOCOL.md](../specifications/JSON_PROTOCOL.md) and `shared/examples/`. The examples are regenerated with `scripts/development/regenerate-examples.sh`.
- [ ] `npm run validate:schemas` passes.
- [ ] The version impact is classified (none, minor or major). A major change uses `!` and a `BREAKING CHANGE:` footer.
- [ ] Renamed settings or commands have a migration note.

## 5. Documentation

- [ ] User-visible changes are reflected in `docs/user/` ([GETTING_STARTED.md](../user/GETTING_STARTED.md), [CONFIGURATION.md](../user/CONFIGURATION.md), [FAQ.md](../user/FAQ.md)).
- [ ] Behaviour changes are reflected in the relevant specification under `docs/specifications/`.
- [ ] Implementation status words (Implemented, Partial, Planned, Not implemented) are accurate everywhere the feature is mentioned, including [ROADMAP.md](ROADMAP.md) and [LIMITATIONS.md](../specifications/LIMITATIONS.md).
- [ ] Architectural decisions have an ADR in [`docs/architecture/DECISIONS/`](../architecture/DECISIONS/).
- [ ] Public C++ headers document their contracts (`///`).

## 6. Security

- [ ] New external input is validated against a schema or limits.
- [ ] No shell-string process spawning. No request-derived compiler flags.
- [ ] No secrets, credentials or personal data in the code, fixtures, logs or CI output.
- [ ] Changes that touch process spawning, file system access, temporary files, webviews, dependencies or CI permissions update [THREAT_MODEL.md](../engineering/THREAT_MODEL.md).
- [ ] A new dependency is justified in the pull request (purpose, licence, maintenance, alternatives) and pinned.

## 7. Review and merge

- [ ] The pull request title and squash commit follow Conventional Commits.
- [ ] At least one approving review from someone other than the author, or a maintainer's approval where [GIT_WORKFLOW.md](../engineering/GIT_WORKFLOW.md#5-review) requires one.
- [ ] All review conversations are resolved.
- [ ] The issue is closed by the merge (`Closes #…`), and any follow-up issues are filed.
