## Summary

<!-- What does this change do and why? Link the issue: Closes #123 -->

## Type of change

- [ ] feat — new capability
- [ ] fix — bug fix
- [ ] docs — documentation only
- [ ] refactor / chore / ci / test

## Analysis correctness (required for analyzer, constraint, estimation or feasibility changes)

- [ ] No complexity, workload, runtime, memory or feasibility value is reported without support; unsupported cases return `unknown` / `unsupported`
- [ ] Bound status and confidence are set independently and justified
- [ ] Assumptions and limitations are recorded on affected findings

## Compatibility

- [ ] No protocol change
- [ ] Protocol change: schemas, `docs/specifications/JSON_PROTOCOL.md` and the version rules were updated

## Verification

<!-- List the commands you ran and their results. Do not tick boxes for checks you did not run. -->

- [ ] `ctest --preset dev` (or `no-clang`)
- [ ] `npm run lint && npm test` in `extension/`
- [ ] New or updated tests cover the change
- [ ] Documentation updated (README, docs/, CHANGELOG) where behaviour changed

## Security

- [ ] No secrets, tokens or machine-specific paths added
- [ ] External input is validated; no shell-string process spawning
