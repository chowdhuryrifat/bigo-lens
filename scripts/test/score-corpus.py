#!/usr/bin/env python3
"""
BigO Lens Validation Corpus Scoring Harness (BL-601).

Scores the BigO Lens static analysis and asymptotic complexity engine against
the representative validation corpus in tests/corpus/.

Usage:
  python3 scripts/test/score-corpus.py [options]

Options:
  --engine PATH        Path to the bigolens-engine binary
  --corpus-dir DIR     Path to the validation corpus directory [default: tests/corpus]
  --manifest PATH      Path to manifest.json [default: <corpus-dir>/manifest.json]
  --output PATH        Path to output JSON results file [optional]
  --markdown PATH      Path to output Markdown summary file [optional]
  --fail-on-regression Exit with non-zero status if any case fails ground truth
"""

import argparse
import json
import os
import subprocess
import sys
from typing import Any, Dict, List, Optional, Tuple


def find_default_engine(repo_root: str) -> Optional[str]:
    """Search common build locations for bigolens-engine."""
    candidates = [
        os.environ.get("BIGOLENS_ENGINE_PATH"),
        os.path.join(repo_root, "build/ci/engine/bigolens-engine"),
        os.path.join(repo_root, "build/dev/engine/bigolens-engine"),
        os.path.join(repo_root, "target/debug/bigolens-engine"),
        os.path.join(repo_root, "target/release/bigolens-engine"),
    ]
    for c in candidates:
        if c and os.path.isfile(c) and os.access(c, os.X_OK):
            return os.path.abspath(c)
    return None


def select_primary_finding(findings: List[Dict[str, Any]]) -> Optional[Dict[str, Any]]:
    """Select the primary algorithmic/time finding from a response."""
    if not findings:
        return None
    # 1. Look for findings with non-null time
    time_findings = [f for f in findings if f.get("time") is not None]
    if time_findings:
        return time_findings[0]
    # 2. Look for algorithmic categories (loop_nest, recursion, stl_algorithm)
    alg_findings = [
        f for f in findings if f.get("category") in ("loop_nest", "recursion", "stl_algorithm")
    ]
    if alg_findings:
        return alg_findings[0]
    # 3. Fallback to first finding
    return findings[0]


def evaluate_case(
    engine_path: str,
    corpus_dir: str,
    case_entry: Dict[str, Any],
) -> Dict[str, Any]:
    """Run engine analysis on a single corpus case and score it against expected."""
    case_id = case_entry["id"]
    name = case_entry["name"]
    src_rel = case_entry["file"]
    src_abs = os.path.join(corpus_dir, src_rel)

    if not os.path.isfile(src_abs):
        return {
            "id": case_id,
            "name": name,
            "status": "error",
            "error": f"Source file not found: {src_abs}",
            "passed": False,
        }

    with open(src_abs, "r", encoding="utf-8") as f:
        content = f.read()

    sidecar_rel = case_entry.get("sidecar")
    constraints = None
    if sidecar_rel:
        sidecar_abs = os.path.join(corpus_dir, sidecar_rel)
        if os.path.isfile(sidecar_abs):
            with open(sidecar_abs, "r", encoding="utf-8") as sc:
                constraints = json.load(sc)
                # Strip out any non-protocol meta keys like $schema
                constraints.pop("$schema", None)

    request: Dict[str, Any] = {
        "schema_version": "1.0",
        "request_id": f"corpus-{case_id}",
        "action": "analyze",
        "source": {
            "path": os.path.basename(src_rel),
            "language": "cpp",
            "content": content,
        },
    }
    if constraints:
        request["constraints"] = constraints

    try:
        proc = subprocess.run(
            [engine_path],
            input=json.dumps(request),
            capture_output=True,
            text=True,
            timeout=15,
        )
    except subprocess.TimeoutExpired:
        return {
            "id": case_id,
            "name": name,
            "status": "timeout",
            "error": "Engine timed out after 15s",
            "passed": False,
        }
    except Exception as e:
        return {
            "id": case_id,
            "name": name,
            "status": "error",
            "error": str(e),
            "passed": False,
        }

    if proc.returncode != 0:
        return {
            "id": case_id,
            "name": name,
            "status": "engine_error",
            "error": f"Engine exited with {proc.returncode}: {proc.stderr.strip()}",
            "passed": False,
        }

    try:
        resp = json.loads(proc.stdout)
    except json.JSONDecodeError as e:
        return {
            "id": case_id,
            "name": name,
            "status": "malformed_output",
            "error": f"Failed to parse engine output as JSON: {e}",
            "passed": False,
        }

    if resp.get("status") != "ok":
        errors = resp.get("errors", [])
        return {
            "id": case_id,
            "name": name,
            "status": "analysis_error",
            "error": f"Response status {resp.get('status')}: {errors}",
            "passed": False,
        }

    findings = resp.get("results", {}).get("findings", [])
    primary = select_primary_finding(findings)

    expected = case_entry.get("expected", {})
    exp_pattern = expected.get("pattern")
    exp_status = expected.get("bound_status")
    exp_asymptotic = expected.get("asymptotic")
    exp_is_unknown = expected.get("is_unknown", False)

    actual_pattern = primary.get("pattern") if primary else None
    actual_status = primary.get("bound_status") if primary else None
    actual_asymptotic = (
        primary.get("time", {}).get("asymptotic")
        if primary and primary.get("time")
        else None
    )

    # Compare expectations
    status_match = actual_status == exp_status
    pattern_match = exp_pattern is None or actual_pattern == exp_pattern
    asymp_match = actual_asymptotic == exp_asymptotic
    unknown_match = (actual_status in ("unknown", "unsupported")) == exp_is_unknown

    passed = status_match and asymp_match and pattern_match

    return {
        "id": case_id,
        "name": name,
        "passed": passed,
        "expected": {
            "pattern": exp_pattern,
            "bound_status": exp_status,
            "asymptotic": exp_asymptotic,
            "is_unknown": exp_is_unknown,
        },
        "actual": {
            "pattern": actual_pattern,
            "bound_status": actual_status,
            "asymptotic": actual_asymptotic,
        },
        "matches": {
            "status": status_match,
            "pattern": pattern_match,
            "asymptotic": asymp_match,
            "unknown": unknown_match,
        },
        "rationale": case_entry.get("rationale", ""),
    }


def compute_aggregate_metrics(results: List[Dict[str, Any]]) -> Dict[str, Any]:
    """Aggregate per-case results into summary metrics."""
    total = len(results)
    if total == 0:
        return {"total": 0, "precision_percent": 0.0}

    passed_count = sum(1 for r in results if r.get("passed", False))
    unknown_count = sum(
        1
        for r in results
        if r.get("actual", {}).get("bound_status") in ("unknown", "unsupported")
    )

    status_counts: Dict[str, int] = {}
    pattern_stats: Dict[str, Dict[str, int]] = {}

    for r in results:
        actual_st = r.get("actual", {}).get("bound_status", "missing")
        status_counts[actual_st] = status_counts.get(actual_st, 0) + 1

        exp_pat = r.get("expected", {}).get("pattern", "unspecified")
        if exp_pat not in pattern_stats:
            pattern_stats[exp_pat] = {"total": 0, "passed": 0}
        pattern_stats[exp_pat]["total"] += 1
        if r.get("passed", False):
            pattern_stats[exp_pat]["passed"] += 1

    precision = (passed_count / total) * 100.0
    unknown_rate = (unknown_count / total) * 100.0

    pattern_summary = {}
    for pat, stats in pattern_stats.items():
        pat_total = stats["total"]
        pat_passed = stats["passed"]
        pat_prec = (pat_passed / pat_total * 100.0) if pat_total > 0 else 0.0
        pattern_summary[pat] = {
            "total": pat_total,
            "passed": pat_passed,
            "precision_percent": round(pat_prec, 2),
        }

    return {
        "total_cases": total,
        "passed_cases": passed_count,
        "failed_cases": total - passed_count,
        "precision_percent": round(precision, 2),
        "unknown_count": unknown_count,
        "unknown_rate_percent": round(unknown_rate, 2),
        "status_distribution": status_counts,
        "pattern_breakdown": pattern_summary,
    }


def format_markdown_report(report: Dict[str, Any]) -> str:
    """Format evaluation results into GitHub Flavored Markdown."""
    meta = report.get("metadata", {})
    summary = report.get("summary", {})
    cases = report.get("cases", [])

    lines = [
        "# BigO Lens Validation Corpus Scoring Report",
        "",
        f"- **Engine**: `{meta.get('engine_path', 'unknown')}`",
        f"- **Corpus**: `{meta.get('corpus_dir', 'tests/corpus')}`",
        f"- **Total Cases**: {summary.get('total_cases', 0)}",
        f"- **Precision / Accuracy**: **{summary.get('precision_percent', 0.0)}%** ({summary.get('passed_cases', 0)} / {summary.get('total_cases', 0)})",
        f"- **Honest Unknown/Unsupported Rate**: **{summary.get('unknown_rate_percent', 0.0)}%** ({summary.get('unknown_count', 0)} cases)",
        "",
        "## Per-Pattern Accuracy Breakdown",
        "",
        "| Pattern | Total | Passed | Precision |",
        "| --- | --- | --- | --- |",
    ]

    for pat, stats in summary.get("pattern_breakdown", {}).items():
        lines.append(
            f"| `{pat}` | {stats['total']} | {stats['passed']} | {stats['precision_percent']}% |"
        )

    lines.extend([
        "",
        "## Detailed Case Results",
        "",
        "| ID | Name | Expected Asymptotic | Actual Asymptotic | Status | Verdict |",
        "| --- | --- | --- | --- | --- | --- |",
    ])

    for c in cases:
        case_id = c.get("id", "")
        name = c.get("name", "")
        exp_asymp = c.get("expected", {}).get("asymptotic") or "—"
        act_asymp = c.get("actual", {}).get("asymptotic") or "—"
        act_st = c.get("actual", {}).get("bound_status") or "error"
        passed = c.get("passed", False)
        verdict = "✅ Pass" if passed else "❌ Fail"
        lines.append(
            f"| {case_id} | {name} | `{exp_asymp}` | `{act_asymp}` | `{act_st}` | {verdict} |"
        )

    return "\n".join(lines) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser(description="BigO Lens Validation Corpus Scorer")
    parser.add_argument("--engine", help="Path to bigolens-engine executable")
    parser.add_argument(
        "--corpus-dir",
        default="tests/corpus",
        help="Path to validation corpus directory",
    )
    parser.add_argument("--manifest", help="Path to corpus manifest.json")
    parser.add_argument("--output", help="Path to write JSON results report")
    parser.add_argument("--markdown", help="Path to write Markdown summary report")
    parser.add_argument(
        "--fail-on-regression",
        action="store_true",
        help="Exit with code 1 if precision is not 100%",
    )

    args = parser.parse_args()

    repo_root = os.path.abspath(
        os.path.join(os.path.dirname(os.path.abspath(__file__)), "../..")
    )
    engine_path = args.engine or find_default_engine(repo_root)
    if not engine_path:
        sys.stderr.write(
            "Error: could not find bigolens-engine. Provide --engine PATH or build the engine first.\n"
        )
        return 2

    corpus_dir = os.path.abspath(args.corpus_dir)
    manifest_path = (
        os.path.abspath(args.manifest)
        if args.manifest
        else os.path.join(corpus_dir, "manifest.json")
    )

    if not os.path.isfile(manifest_path):
        sys.stderr.write(f"Error: manifest file not found: {manifest_path}\n")
        return 2

    with open(manifest_path, "r", encoding="utf-8") as f:
        manifest = json.load(f)

    case_entries = manifest.get("cases", [])
    if not case_entries:
        sys.stderr.write(f"Error: no test cases found in manifest: {manifest_path}\n")
        return 2

    print(f"Scoring {len(case_entries)} corpus cases against engine: {engine_path}")
    case_results = []
    for c in case_entries:
        res = evaluate_case(engine_path, corpus_dir, c)
        case_results.append(res)
        mark = "✓" if res.get("passed") else "✗"
        print(f"  [{mark}] {res['id']}: {res['name']} ({res.get('actual', {}).get('asymptotic') or res.get('actual', {}).get('bound_status')})")

    summary = compute_aggregate_metrics(case_results)

    report = {
        "metadata": {
            "engine_path": engine_path,
            "corpus_dir": corpus_dir,
            "manifest_path": manifest_path,
        },
        "summary": summary,
        "cases": case_results,
    }

    print("\n--- Summary ---")
    print(f"Total Cases:     {summary['total_cases']}")
    print(f"Passed:          {summary['passed_cases']}")
    print(f"Precision:       {summary['precision_percent']}%")
    print(f"Honest Unknown:  {summary['unknown_count']} ({summary['unknown_rate_percent']}%)")

    if args.output:
        os.makedirs(os.path.dirname(os.path.abspath(args.output)), exist_ok=True)
        with open(args.output, "w", encoding="utf-8") as f:
            json.dump(report, f, indent=2)
        print(f"Wrote JSON report to: {args.output}")

    if args.markdown:
        os.makedirs(os.path.dirname(os.path.abspath(args.markdown)), exist_ok=True)
        md_text = format_markdown_report(report)
        with open(args.markdown, "w", encoding="utf-8") as f:
            f.write(md_text)
        print(f"Wrote Markdown report to: {args.markdown}")

    step_summary_path = os.environ.get("GITHUB_STEP_SUMMARY")
    if step_summary_path:
        with open(step_summary_path, "a", encoding="utf-8") as f:
            f.write(format_markdown_report(report))

    if args.fail_on_regression and summary["failed_cases"] > 0:
        sys.stderr.write(
            f"Regression failure: {summary['failed_cases']} case(s) failed evaluation.\n"
        )
        return 1

    return 0


if __name__ == "__main__":
    sys.exit(main())
