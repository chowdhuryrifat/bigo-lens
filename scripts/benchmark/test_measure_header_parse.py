#!/usr/bin/env python3
"""
Unit tests for BigO Lens Header Parsing & Precompiled Preamble Benchmark (BL-607).

Validates:
  - Statistical and summary metrics calculations (median, mean, min, max, std, mad)
  - Speedup and time-saving derivations
  - Compiler discovery and argument builder logic
  - Markdown and JSON output formatting
  - Mocked end-to-end benchmark suite execution
"""

import json
import os
import sys
import tempfile
import unittest
from unittest.mock import MagicMock, patch

# Add benchmark directory to path
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from importlib.util import module_from_spec, spec_from_file_location

measure_script_path = os.path.join(
    os.path.dirname(os.path.abspath(__file__)), "measure-header-parse.py"
)
spec = spec_from_file_location("measure_header_parse", measure_script_path)
if spec is None or spec.loader is None:
    raise ImportError(f"Cannot load {measure_script_path}")
measure = module_from_spec(spec)
sys.modules["measure_header_parse"] = measure
spec.loader.exec_module(measure)


class TestSummaryStats(unittest.TestCase):
    """Test statistical aggregation functions."""

    def test_compute_stats_empty(self):
        stats = measure.compute_stats([])
        self.assertEqual(stats["median"], 0.0)
        self.assertEqual(stats["mean"], 0.0)
        self.assertEqual(stats["min"], 0.0)
        self.assertEqual(stats["max"], 0.0)
        self.assertEqual(stats["std"], 0.0)
        self.assertEqual(stats["mad"], 0.0)

    def test_compute_stats_single(self):
        stats = measure.compute_stats([42.5])
        self.assertEqual(stats["median"], 42.5)
        self.assertEqual(stats["mean"], 42.5)
        self.assertEqual(stats["min"], 42.5)
        self.assertEqual(stats["max"], 42.5)
        self.assertEqual(stats["std"], 0.0)
        self.assertEqual(stats["mad"], 0.0)

    def test_compute_stats_multiple(self):
        samples = [10.0, 20.0, 30.0, 40.0, 50.0]
        stats = measure.compute_stats(samples)
        self.assertEqual(stats["median"], 30.0)
        self.assertEqual(stats["mean"], 30.0)
        self.assertEqual(stats["min"], 10.0)
        self.assertEqual(stats["max"], 50.0)
        self.assertAlmostEqual(stats["std"], 15.81, places=2)
        # Deviations: [20, 10, 0, 10, 20] -> median is 10.0
        self.assertEqual(stats["mad"], 10.0)


class TestSpeedupCalculations(unittest.TestCase):
    """Test speedup factor and time savings calculations."""

    def test_compute_speedup_normal(self):
        cold = {"median": 400.0}
        warm = {"median": 80.0}
        cold_cpu = {"median": 390.0}
        warm_cpu = {"median": 75.0}

        metrics = measure.compute_speedup(cold, warm, cold_cpu, warm_cpu)
        self.assertEqual(metrics["speedup_factor"], 5.0)
        self.assertEqual(metrics["cpu_speedup_factor"], 5.2)
        self.assertEqual(metrics["time_saved_ms"], 320.0)
        self.assertEqual(metrics["time_saved_percent"], 80.0)

    def test_compute_speedup_near_zero_warm(self):
        cold = {"median": 100.0}
        warm = {"median": 0.0}
        cold_cpu = {"median": 100.0}
        warm_cpu = {"median": 0.0}

        metrics = measure.compute_speedup(cold, warm, cold_cpu, warm_cpu)
        self.assertEqual(metrics["speedup_factor"], 1.0)
        self.assertEqual(metrics["time_saved_ms"], 100.0)

    def test_compute_speedup_warm_slower(self):
        cold = {"median": 30.0}
        warm = {"median": 36.0}
        cold_cpu = {"median": 30.0}
        warm_cpu = {"median": 35.0}

        metrics = measure.compute_speedup(cold, warm, cold_cpu, warm_cpu)
        self.assertEqual(metrics["speedup_factor"], 0.83)
        self.assertEqual(metrics["time_saved_ms"], -6.0)
        self.assertEqual(metrics["time_saved_percent"], -20.0)


class TestCommandBuilder(unittest.TestCase):
    """Test CLI command line generation for Clang invocations."""

    def test_build_pch_compile_cmd(self):
        cmd = measure.build_pch_compile_cmd(
            "/usr/bin/clang++", "header.hpp", "header.pch", std="c++20"
        )
        self.assertEqual(cmd[0], "/usr/bin/clang++")
        self.assertIn("-std=c++20", cmd)
        self.assertIn("-x", cmd)
        self.assertIn("c++-header", cmd)
        self.assertIn("-emit-pch", cmd)
        self.assertIn("-o", cmd)
        self.assertIn("header.pch", cmd)

    def test_build_cold_parse_cmd(self):
        cmd = measure.build_cold_parse_cmd(
            "/usr/bin/clang++", "source.cpp", std="c++20"
        )
        self.assertEqual(cmd[0], "/usr/bin/clang++")
        self.assertIn("-std=c++20", cmd)
        self.assertIn("-fsyntax-only", cmd)
        self.assertIn("source.cpp", cmd)

    def test_build_warm_parse_cmd(self):
        cmd = measure.build_warm_parse_cmd(
            "/usr/bin/clang++", "header.pch", "source.cpp", std="c++20"
        )
        self.assertEqual(cmd[0], "/usr/bin/clang++")
        self.assertIn("-std=c++20", cmd)
        self.assertIn("-fsyntax-only", cmd)
        self.assertIn("-include-pch", cmd)
        self.assertIn("header.pch", cmd)
        self.assertIn("source.cpp", cmd)


class TestClangDiscovery(unittest.TestCase):
    """Test compiler binary resolution."""

    def test_find_clang_explicit_nonexistent(self):
        with self.assertRaises(FileNotFoundError):
            measure.find_clang_compiler("/nonexistent/clang/path/that/does/not/exist")

    @patch("shutil.which")
    @patch("os.path.isfile")
    @patch("os.access")
    def test_find_clang_from_env(self, mock_access, mock_isfile, mock_which):
        mock_access.return_value = True
        mock_isfile.return_value = True
        mock_which.return_value = "/opt/custom/clang++"

        with patch.dict(os.environ, {"CLANGXX": "/opt/custom/clang++"}):
            found = measure.find_clang_compiler()
            self.assertEqual(found, "/opt/custom/clang++")


class TestFormattingHelpers(unittest.TestCase):
    """Test string and markdown formatting."""

    def test_format_size(self):
        self.assertEqual(measure.format_size(500), "500 B")
        self.assertEqual(measure.format_size(1536), "1.5 KiB")
        self.assertEqual(measure.format_size(10485760), "10.00 MiB")

    def test_generate_markdown_report(self):
        dummy_data = {
            "metadata": {
                "timestamp": "2026-11-14T20:00:00Z",
                "clang_path": "/usr/bin/clang++-20",
                "clang_version": "Ubuntu clang version 20.1.0",
                "platform": {
                    "system": "Linux",
                    "release": "6.8.0",
                    "machine": "x86_64",
                    "processor": "x86_64",
                },
                "iterations": 5,
                "cpp_standard": "c++20",
            },
            "summary": {
                "total_cold_parse_median_ms": 500.0,
                "total_warm_parse_median_ms": 100.0,
                "overall_speedup_factor": 5.0,
                "total_pch_size_bytes": 10485760,
                "total_pch_size_formatted": "10.00 MiB",
            },
            "suites": {
                "containers": {
                    "id": "containers",
                    "name": "Containers suite",
                    "description": "<vector>, <set>, <map>, <string>",
                    "headers": ["<vector>", "<set>", "<map>", "<string>"],
                    "pch_size_bytes": 4194304,
                    "pch_size_formatted": "4.00 MiB",
                    "cold_parse": {
                        "wall_ms": {"median": 250.0, "mad": 5.0},
                    },
                    "pch_compile": {
                        "wall_ms": {"median": 400.0},
                    },
                    "warm_parse": {
                        "wall_ms": {"median": 50.0, "mad": 2.0},
                    },
                    "speedup": {
                        "speedup_factor": 5.0,
                    },
                }
            },
        }

        md = measure.generate_markdown_report(dummy_data)
        self.assertIn("# Header Parse Cost & Precompiled Preamble Benchmark (BL-607)", md)
        self.assertIn("Containers suite", md)
        self.assertIn("`500.0 ms`", md)
        self.assertIn("5.00x", md)
        self.assertIn("10.00 MiB", md)


class TestMockBenchmarkExecution(unittest.TestCase):
    """Test full benchmark workflow under mocked subprocess calls."""

    @patch("measure_header_parse.os.path.getsize")
    @patch("measure_header_parse.os.path.exists")
    @patch("measure_header_parse.run_timed_command")
    @patch("measure_header_parse.get_clang_version")
    def test_run_all_benchmarks_mocked(
        self, mock_version, mock_run, mock_exists, mock_getsize
    ):
        mock_version.return_value = "Clang mock version 20.0.0"
        mock_exists.return_value = True
        mock_getsize.return_value = 1048576  # 1 MiB

        # Wall: 100ms cold, 20ms warm, 200ms pch
        mock_run.return_value = (100.0, 95.0, 0, "")

        results = measure.run_all_benchmarks(
            clang_path="/mock/clang++",
            iterations=2,
            std="c++20",
            verbose=False,
        )

        self.assertIn("metadata", results)
        self.assertIn("summary", results)
        self.assertIn("suites", results)

        # Verify all 5 required suites are present
        expected_suites = [
            "minimal",
            "algorithms",
            "containers",
            "iostreams",
            "competitive_bundle",
        ]
        for s in expected_suites:
            self.assertIn(s, results["suites"])
            suite_data = results["suites"][s]
            self.assertIn("cold_parse", suite_data)
            self.assertIn("pch_compile", suite_data)
            self.assertIn("warm_parse", suite_data)
            self.assertIn("speedup", suite_data)
            self.assertIn("pch_size_bytes", suite_data)


if __name__ == "__main__":
    unittest.main()
