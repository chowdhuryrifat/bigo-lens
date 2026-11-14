# Maintainer Notes

Operational and repository configuration notes for BigO Lens maintainers.

## 1. Branch Protection & GitHub Rulesets (BL-107)

The `main` branch is governed by a GitHub repository ruleset rather than legacy branch protection:

- **Ruleset ID**: `27279054` (`Protect main`)
- **Enforcement**: `Active`
- **Target**: Default branch (`~DEFAULT_BRANCH`)
- **Bypass Actors**: None (`current_user_can_bypass: never`)
- **Configured Rules**:
  - `deletion`: Restricts branch deletion.
  - `non_fast_forward`: Restricts force pushes (`git push --force`).
  - `pull_request`: Requires all changes to arrive through pull requests. Direct pushes are rejected with GitHub error `GH013`. Requires review thread resolution and dismisses stale approvals on new pushes.
  - `required_status_checks`: Strict policy requiring branches to be rebased up-to-date with latest `main` before merging, with 8 required CI contexts:
    - `CI/Rust · Workspace checks & tests`
    - `Code quality/Rust · Format & Clippy`
    - `CI/Native · Ubuntu · Clang frontend`
    - `CI/Extension · ubuntu-24.04`
    - `Code quality/PR Branch & Conventions Check`
    - `Code quality/clang-format`
    - `Code quality/clang-tidy (Ubuntu 24.04, Clang 18)`
    - `Code quality/typescript (ESLint and Prettier)`

### Modifying or Inspecting Rulesets
To inspect the live ruleset:
```bash
gh api /repos/chowdhuryrifat/bigo-lens/rulesets/27279054
```

To update rules:
```bash
gh api -X PUT /repos/chowdhuryrifat/bigo-lens/rulesets/27279054 --input payload.json
```

---

## 2. Security and Vulnerability Reporting

- **Private Vulnerability Reporting**: Enabled on the repository. Security researchers can submit advisories privately via `Security -> Advisories -> Report a vulnerability`.
- **Dependabot Security Updates & Alerts**: Enabled. Automated pull requests are opened for vulnerable npm or cargo dependencies.
- **Secret Scanning & Push Protection**: Enabled across all branches.

---

## 3. Issue and PR Templates

Templates are maintained in `.github/ISSUE_TEMPLATE/`:
- `bug_report.yml`: Structured form for reporting incorrect complexity/workload results, crashes, or extension errors.
- `feature_request.yml`: Structured form with product principle checkboxes enforcing non-fabrication and offline operation.
- `documentation.yml`: Targeted form for reporting documentation gaps, discrepancies, or broken references.
- `config.yml`: Sets `blank_issues_enabled: false` and routes security disclosures to the private advisory pipeline.
- `.github/PULL_REQUEST_TEMPLATE.md`: Standard PR checklist requiring branch naming compliance, Conventional Commits, testing evidence, and documentation updates.
