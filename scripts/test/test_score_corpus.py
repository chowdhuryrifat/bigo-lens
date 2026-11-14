#!/usr/bin/env python3
"""
Unit tests for the validation corpus scoring harness (scripts/test/score-corpus.py).
Tests aggregate calculations, finding selection, reporting, and synthetic evaluations.
"""

import json
import os
import sys
import tempfile
import unittest
from unittest.mock import MagicMock, patch

# Add scripts/test to Python path
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from importlib.util import module_from_spec, spec_from_file_location

score_corpus_path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "score-corpus.py")
spec = spec_from_file_location("score_corpus", score_corpus_path)
if spec is None or spec.loader is None:
    raise ImportError(f"Cannot load {score_corpus_path}")
score_corpus = module_from_spec(spec)
spec.loader.exec_module(score_corpus)


class TestScoreCorpusMetrics(unittest.TestCase):
    def test_compute_aggregate_metrics_empty(self):
        metrics = score_corpus.compute_aggregate_metrics([])
        self.assertEqual(metrics["total"], 0)
        self.assertEqual(metrics["precision_percent"], 0.0)

    def test_compute_aggregate_metrics_perfect(self):
        results = [
            {
                "id": "C1",
                "passed": True,
                "expected": {"pattern": "linear-loop", "bound_status": "proven"},
                "actual": {"bound_status": "proven"},
            },
            {
                "id": "C2",
                "passed": True,
                "expected": {"pattern": "nested-loop", "bound_status": "proven"},
                "actual": {"bound_status": "proven"},
            },
            {
                "id": "C3",
                "passed": True,
                "expected": {"pattern": "data-dependent-loop", "bound_status": "unknown"},
                "actual": {"bound_status": "unknown"},
            },
        ]
        metrics = score_corpus.compute_aggregate_metrics(results)
        self.assertEqual(metrics["total_cases"], 3)
        self.assertEqual(metrics["passed_cases"], 3)
        self.assertEqual(metrics["failed_cases"], 0)
        self.assertEqual(metrics["precision_percent"], 100.0)
        self.assertEqual(metrics["unknown_count"], 1)
        self.assertAlmostEqual(metrics["unknown_rate_percent"], 33.33, places=2)
        self.assertEqual(metrics["status_distribution"]["proven"], 2)
        self.assertEqual(metrics["status_distribution"]["unknown"], 1)
        self.assertEqual(metrics["pattern_breakdown"]["linear-loop"]["precision_percent"], 100.0)

    def test_compute_aggregate_metrics_with_failure(self):
        results = [
            {
                "id": "C1",
                "passed": True,
                "expected": {"pattern": "linear-loop"},
                "actual": {"bound_status": "proven"},
            },
            {
                "id": "C2",
                "passed": False,
                "expected": {"pattern": "linear-loop"},
                "actual": {"bound_status": "unknown"},
            },
        ]
        metrics = score_corpus.compute_aggregate_metrics(results)
        self.assertEqual(metrics["total_cases"], 2)
        self.assertEqual(metrics["passed_cases"], 1)
        self.assertEqual(metrics["failed_cases"], 1)
        self.assertEqual(metrics["precision_percent"], 50.0)
        self.assertEqual(metrics["pattern_breakdown"]["linear-loop"]["precision_percent"], 50.0)


class TestFindingSelection(unittest.TestCase):
    def test_select_primary_finding_time_preferred(self):
        findings = [
            {"id": "F1", "category": "static_storage", "pattern": "static-array", "time": None},
            {"id": "F2", "category": "loop_nest", "pattern": "linear-loop", "time": {"asymptotic": "O(n)"}},
        ]
        selected = score_corpus.select_primary_finding(findings)
        self.assertIsNotNone(selected)
        self.assertEqual(selected["id"], "F2")

    def test_select_primary_finding_algorithm_fallback(self):
        findings = [
            {"id": "F1", "category": "static_storage", "pattern": "static-array", "time": None},
            {"id": "F2", "category": "loop_nest", "pattern": "data-dependent-loop", "time": None},
        ]
        selected = score_corpus.select_primary_finding(findings)
        self.assertIsNotNone(selected)
        self.assertEqual(selected["id"], "F2")

    def test_select_primary_finding_empty(self):
        self.assertIsNone(score_corpus.select_primary_finding([]))


class TestMarkdownReport(unittest.TestCase):
    def test_format_markdown_report(self):
        report = {
            "metadata": {
                "engine_path": "/bin/bigolens-engine",
                "corpus_dir": "tests/corpus",
            },
            "summary": {
                "total_cases": 1,
                "passed_cases": 1,
                "failed_cases": 0,
                "precision_percent": 100.0,
                "unknown_count": 0,
                "unknown_rate_percent": 0.0,
                "pattern_breakdown": {
                    "linear-loop": {"total": 1, "passed": 1, "precision_percent": 100.0}
                },
            },
            "cases": [
                {
                    "id": "CORPUS-01",
                    "name": "Prefix Sums",
                    "passed": True,
                    "expected": {"asymptotic": "O(n)"},
                    "actual": {"asymptotic": "O(n)", "bound_status": "proven"},
                }
            ],
        }
        md = score_corpus.format_markdown_report(report)
        self.assertIn("# BigO Lens Validation Corpus Scoring Report", md)
        self.assertIn("100.0%", md)
        self.assertIn("CORPUS-01", md)
        self.assertIn("✅ Pass", md)


class TestSyntheticCorpusEvaluation(unittest.TestCase):
    def test_evaluate_case_synthetic(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            src_file = os.path.join(tmpdir, "test.cpp")
            with open(src_file, "w") as f:
                f.write("int main() { return 0; }\n")

            case_entry = {
                "id": "SYNTH-01",
                "name": "Synthetic Test",
                "file": "test.cpp",
                "expected": {
                    "pattern": "linear-loop",
                    "bound_status": "proven",
                    "asymptotic": "O(n)",
                    "is_unknown": False,
                },
            }

            fake_response = {
                "schema_version": "1.0",
                "status": "ok",
                "results": {
                    "findings": [
                        {
                            "id": "F1",
                            "category": "loop_nest",
                            "pattern": "linear-loop",
                            "bound_status": "proven",
                            "time": {"asymptotic": "O(n)"},
                        }
                    ]
                },
            }

            with patch("subprocess.run") as mock_run:
                mock_run.return_value = MagicMock(
                    returncode=0,
                    stdout=json.dumps(fake_response),
                    stderr="",
                )
                res = score_corpus.evaluate_case("/fake/engine", tmpdir, case_entry)
                self.assertTrue(res["passed"])
                self.assertEqual(res["actual"]["asymptotic"], "O(n)")
                self.assertEqual(res["actual"]["bound_status"], "proven")


if __name__ == "__main__":
    unittest.main()
