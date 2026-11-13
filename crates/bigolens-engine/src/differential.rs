//! Differential test harness module.
//!
//! Compares static asymptotic classes with measured empirical growth fits
//! for controlled fixture programs (Linear, n log n, quadratic).
//! Conforms to BL-408 and TESTING_STRATEGY.md §2.5.

use crate::{analyze_action, benchmark_action};
use bigolens_core::api::{BenchmarkConfig, Request, SourcePayload, PROTOCOL_VERSION};
use bigolens_core::constraints::{ConstraintSet, VariableBound};
use bigolens_core::growth_fit::GrowthModel;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Verdict of a differential test comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DifferentialVerdict {
    /// Confident empirical model matches static/expected asymptotic class.
    Matched,
    /// Empirical growth fit is ambiguous (does not fail harness).
    Ambiguous,
    /// Confident empirical model disagrees with static/expected class.
    Mismatch,
    /// Compilation or benchmark runner error occurred.
    Error,
}

impl std::fmt::Display for DifferentialVerdict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Matched => write!(f, "Matched"),
            Self::Ambiguous => write!(f, "Ambiguous"),
            Self::Mismatch => write!(f, "Mismatch"),
            Self::Error => write!(f, "Error"),
        }
    }
}

/// A controlled program case to evaluate differentially.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DifferentialCase {
    pub name: String,
    pub path: String,
    pub source: String,
    pub language: String,
    pub constraints: ConstraintSet,
    pub benchmark_config: BenchmarkConfig,
    /// Explicit expected asymptotic string or model override for testing harness sensitivity.
    pub expected_class: Option<String>,
}

/// Result of evaluating a single differential test case.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseResult {
    pub name: String,
    pub path: String,
    pub static_asymptotic: Option<String>,
    pub expected_model: Option<GrowthModel>,
    pub empirical_model: Option<GrowthModel>,
    pub ambiguous: bool,
    pub r_squared: Option<f64>,
    pub rss: Option<f64>,
    pub verdict: DifferentialVerdict,
    pub message: String,
}

/// Full differential test report summarizing all evaluated cases.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DifferentialReport {
    pub total: usize,
    pub matched: usize,
    pub ambiguous: usize,
    pub mismatched: usize,
    pub errors: usize,
    pub passed: bool,
    pub cases: Vec<CaseResult>,
}

/// Maps asymptotic Big-O strings (e.g. "O(n)", "O(n log n)", "O(n^2)") to `GrowthModel`.
pub fn map_asymptotic_to_growth_model(asymptotic: &str) -> Option<GrowthModel> {
    let lower = asymptotic.trim().to_lowercase();
    let s = lower
        .strip_prefix("o(")
        .and_then(|s| s.strip_suffix(')'))
        .unwrap_or(&lower)
        .trim();

    if s == "linearithmic"
        || s == "n log n"
        || s == "n * log n"
        || s == "n*log(n)"
        || s == "n * log_2(n)"
        || s == "n*log_2(n)"
        || (s.contains("log") && (s.starts_with('n') || s.contains("linear")))
    {
        Some(GrowthModel::Linearithmic)
    } else if s == "quadratic"
        || s == "n^2"
        || s == "n * n"
        || s == "n*n"
        || s.contains("quadratic")
        || s.contains("n^2")
    {
        Some(GrowthModel::Quadratic)
    } else if s == "linear" || s == "n" || s == "n + 1" || s == "n - 1" {
        Some(GrowthModel::Linear)
    } else {
        None
    }
}

/// Extracts the asymptotic notation string from the dominant finding in an analyze response.
pub fn extract_dominant_asymptotic(results: &serde_json::Value) -> Option<String> {
    let dominant_id = results
        .get("summary")
        .and_then(|s| s.get("dominant_finding_id"))
        .and_then(|id| id.as_str());

    let findings = results.get("findings").and_then(|f| f.as_array())?;

    if let Some(dom_id) = dominant_id {
        for f in findings {
            if f.get("id").and_then(|id| id.as_str()) == Some(dom_id) {
                if let Some(asy) = f
                    .get("time")
                    .and_then(|t| t.get("asymptotic"))
                    .and_then(|a| a.as_str())
                {
                    return Some(asy.to_string());
                }
            }
        }
    }

    for f in findings {
        if let Some(asy) = f
            .get("time")
            .and_then(|t| t.get("asymptotic"))
            .and_then(|a| a.as_str())
        {
            return Some(asy.to_string());
        }
    }

    None
}

/// Evaluate a single differential test case.
pub fn evaluate_case(case: &DifferentialCase) -> CaseResult {
    // 1. Static analysis
    let req_analyze = Request {
        schema_version: PROTOCOL_VERSION.to_string(),
        request_id: format!("diff-analyze-{}", case.name),
        action: "analyze".to_string(),
        source: Some(SourcePayload {
            path: case.path.clone(),
            language: case.language.clone(),
            content: case.source.clone(),
        }),
        constraints: Some(case.constraints.clone()),
        benchmark_config: None,
        options: None,
    };

    let analyze_resp = analyze_action(req_analyze);
    if analyze_resp.status != "ok" {
        let msg = analyze_resp
            .errors
            .first()
            .map(|e| e.message.clone())
            .unwrap_or_else(|| "static analysis failed".to_string());
        return CaseResult {
            name: case.name.clone(),
            path: case.path.clone(),
            static_asymptotic: None,
            expected_model: None,
            empirical_model: None,
            ambiguous: false,
            r_squared: None,
            rss: None,
            verdict: DifferentialVerdict::Error,
            message: format!("Static analysis error: {}", msg),
        };
    }

    let results = analyze_resp.results.unwrap_or_default();
    let static_asymptotic = if let Some(ref exp) = case.expected_class {
        Some(exp.clone())
    } else {
        extract_dominant_asymptotic(&results)
    };

    let expected_model = static_asymptotic
        .as_deref()
        .and_then(map_asymptotic_to_growth_model);

    // 2. Dynamic benchmark
    let req_bench = Request {
        schema_version: PROTOCOL_VERSION.to_string(),
        request_id: format!("diff-bench-{}", case.name),
        action: "benchmark".to_string(),
        source: Some(SourcePayload {
            path: case.path.clone(),
            language: case.language.clone(),
            content: case.source.clone(),
        }),
        constraints: Some(case.constraints.clone()),
        benchmark_config: Some(case.benchmark_config.clone()),
        options: None,
    };

    let bench_resp = benchmark_action(req_bench);
    if bench_resp.status != "ok" {
        let msg = bench_resp
            .errors
            .first()
            .map(|e| e.message.clone())
            .unwrap_or_else(|| "benchmark run failed".to_string());
        return CaseResult {
            name: case.name.clone(),
            path: case.path.clone(),
            static_asymptotic,
            expected_model,
            empirical_model: None,
            ambiguous: false,
            r_squared: None,
            rss: None,
            verdict: DifferentialVerdict::Error,
            message: format!("Benchmark error: {}", msg),
        };
    }

    let bench_results = bench_resp.results.unwrap_or_default();
    let bench_obj = &bench_results["benchmark"];
    let fit_obj = &bench_obj["empirical_fit"];

    let ambiguous = fit_obj["ambiguous"].as_bool().unwrap_or(true);
    let empirical_model_str = fit_obj["model"].as_str();
    let empirical_model = empirical_model_str.and_then(map_asymptotic_to_growth_model);
    let r_squared = fit_obj["r_squared"].as_f64();
    let rss = fit_obj["rss"].as_f64();

    // 3. Compare static and empirical model
    let (verdict, message) = determine_verdict(
        expected_model,
        empirical_model,
        ambiguous,
        static_asymptotic.as_deref(),
    );

    CaseResult {
        name: case.name.clone(),
        path: case.path.clone(),
        static_asymptotic,
        expected_model,
        empirical_model,
        ambiguous,
        r_squared,
        rss,
        verdict,
        message,
    }
}

/// Determines the differential verdict and explanatory message from comparison inputs.
pub fn determine_verdict(
    expected_model: Option<GrowthModel>,
    empirical_model: Option<GrowthModel>,
    ambiguous: bool,
    static_asymptotic: Option<&str>,
) -> (DifferentialVerdict, String) {
    if ambiguous {
        (
            DifferentialVerdict::Ambiguous,
            format!(
                "Empirical fit is ambiguous for candidate fits (static: {:?})",
                static_asymptotic
            ),
        )
    } else {
        match (expected_model, empirical_model) {
            (Some(exp), Some(emp)) if exp == emp => (
                DifferentialVerdict::Matched,
                format!(
                    "Confident empirical model {:?} matches expected {:?}",
                    emp, exp
                ),
            ),
            (Some(exp), Some(emp)) => (
                DifferentialVerdict::Mismatch,
                format!(
                    "Confident empirical model {:?} mismatches expected {:?}",
                    emp, exp
                ),
            ),
            (None, Some(emp)) => (
                DifferentialVerdict::Mismatch,
                format!(
                    "Empirical fit {:?} could not be compared: unknown static asymptotic {:?}",
                    emp, static_asymptotic
                ),
            ),
            (Some(exp), None) => (
                DifferentialVerdict::Mismatch,
                format!("Expected model {:?} but no empirical model selected", exp),
            ),
            (None, None) => (
                DifferentialVerdict::Mismatch,
                "Neither expected nor empirical model could be identified".to_string(),
            ),
        }
    }
}

/// Executes all provided differential test cases and produces a structured report.
pub fn run_differential_harness(cases: &[DifferentialCase]) -> DifferentialReport {
    let mut results = Vec::with_capacity(cases.len());
    let mut matched = 0;
    let mut ambiguous = 0;
    let mut mismatched = 0;
    let mut errors = 0;

    for case in cases {
        let res = evaluate_case(case);
        match res.verdict {
            DifferentialVerdict::Matched => matched += 1,
            DifferentialVerdict::Ambiguous => ambiguous += 1,
            DifferentialVerdict::Mismatch => mismatched += 1,
            DifferentialVerdict::Error => errors += 1,
        }
        results.push(res);
    }

    // A confident disagreement or error fails the harness. Ambiguous fits do not fail.
    let passed = mismatched == 0 && errors == 0;

    DifferentialReport {
        total: cases.len(),
        matched,
        ambiguous,
        mismatched,
        errors,
        passed,
        cases: results,
    }
}

/// Writes the differential report to disk as formatted JSON.
pub fn write_report(report: &DifferentialReport, output_path: &Path) -> std::io::Result<()> {
    if let Some(parent) = output_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let json_bytes = serde_json::to_string_pretty(report)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    fs::write(output_path, json_bytes)?;
    Ok(())
}

const EMBEDDED_SORTING_SOURCE: &str = r#"#include <algorithm>
#include <iostream>
#include <vector>

int main() {
  int n = 0;
  if (!(std::cin >> n)) return 0;
  std::vector<int> values(n);
  unsigned int seed = 42;
  for (int i = 0; i < n; ++i) {
    seed = seed * 1664525u + 1013904223u;
    values[i] = static_cast<int>(seed);
  }
  std::sort(values.begin(), values.end());
  std::cout << values.front() << '\n';
  return 0;
}
"#;

/// Returns the curated controlled test cases for Linear, N log N, and Quadratic complexity.
pub fn default_controlled_cases() -> Vec<DifferentialCase> {
    let mut cases = Vec::new();

    // 1. Linear loop: O(n)
    let linear_source = r#"#include <iostream>

int main() {
    int n = 0;
    if (!(std::cin >> n)) return 0;
    long long sum = 0;
    for (int i = 0; i < n; ++i) {
        sum += (i * 3 + 1) ^ (sum >> 2);
    }
    std::cout << sum << "\n";
    return 0;
}
"#;
    let mut linear_vars = HashMap::new();
    linear_vars.insert("n".to_string(), VariableBound::range(1.0, 20_000_000.0));
    cases.push(DifferentialCase {
        name: "linear_loop".to_string(),
        path: "linear_loop.cpp".to_string(),
        source: linear_source.to_string(),
        language: "cpp".to_string(),
        constraints: ConstraintSet {
            variables: linear_vars,
            time_limit_seconds: Some(2.0),
            memory_limit_mb: Some(256.0),
            ..Default::default()
        },
        benchmark_config: BenchmarkConfig {
            sizes: vec![2_000_000, 4_000_000, 8_000_000, 16_000_000],
            repetitions: 3,
            warmup: 1,
            allow_unsandboxed: true,
            ..Default::default()
        },
        expected_class: None,
    });

    // 2. N log N: sorting
    let sorting_source = EMBEDDED_SORTING_SOURCE.to_string();

    let mut sort_vars = HashMap::new();
    sort_vars.insert("n".to_string(), VariableBound::range(1.0, 200_000.0));
    cases.push(DifferentialCase {
        name: "sorting_n_log_n".to_string(),
        path: "sorting_n_log_n.cpp".to_string(),
        source: sorting_source,
        language: "cpp".to_string(),
        constraints: ConstraintSet {
            variables: sort_vars,
            time_limit_seconds: Some(2.0),
            memory_limit_mb: Some(256.0),
            ..Default::default()
        },
        benchmark_config: BenchmarkConfig {
            sizes: vec![10_000, 20_000, 40_000, 80_000],
            repetitions: 3,
            warmup: 1,
            allow_unsandboxed: true,
            ..Default::default()
        },
        expected_class: None,
    });

    // 3. Quadratic: nested vector loops: O(n^2)
    let quadratic_source = r#"#include <iostream>
#include <vector>

int main() {
    int n = 0;
    if (!(std::cin >> n)) return 0;
    std::vector<int> v(n, 1);
    long long acc = 0;
    for (int i = 0; i < n; ++i) {
        for (int j = 0; j < n; ++j) {
            acc += v[j];
        }
    }
    std::cout << acc << "\n";
    return 0;
}
"#;
    let mut quad_vars = HashMap::new();
    quad_vars.insert("n".to_string(), VariableBound::range(1.0, 20_000.0));
    cases.push(DifferentialCase {
        name: "quadratic_nested_loops".to_string(),
        path: "quadratic_nested.cpp".to_string(),
        source: quadratic_source.to_string(),
        language: "cpp".to_string(),
        constraints: ConstraintSet {
            variables: quad_vars,
            time_limit_seconds: Some(2.0),
            memory_limit_mb: Some(256.0),
            ..Default::default()
        },
        benchmark_config: BenchmarkConfig {
            sizes: vec![2_000, 4_000, 8_000, 16_000],
            repetitions: 3,
            warmup: 1,
            allow_unsandboxed: true,
            ..Default::default()
        },
        expected_class: None,
    });

    cases
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler;

    #[test]
    fn test_map_asymptotic_to_growth_model() {
        assert_eq!(
            map_asymptotic_to_growth_model("O(n)"),
            Some(GrowthModel::Linear)
        );
        assert_eq!(
            map_asymptotic_to_growth_model("n"),
            Some(GrowthModel::Linear)
        );
        assert_eq!(
            map_asymptotic_to_growth_model("linear"),
            Some(GrowthModel::Linear)
        );

        assert_eq!(
            map_asymptotic_to_growth_model("O(n log n)"),
            Some(GrowthModel::Linearithmic)
        );
        assert_eq!(
            map_asymptotic_to_growth_model("O(n * log_2(n))"),
            Some(GrowthModel::Linearithmic)
        );
        assert_eq!(
            map_asymptotic_to_growth_model("linearithmic"),
            Some(GrowthModel::Linearithmic)
        );

        assert_eq!(
            map_asymptotic_to_growth_model("O(n^2)"),
            Some(GrowthModel::Quadratic)
        );
        assert_eq!(
            map_asymptotic_to_growth_model("O(n * n)"),
            Some(GrowthModel::Quadratic)
        );
        assert_eq!(
            map_asymptotic_to_growth_model("quadratic"),
            Some(GrowthModel::Quadratic)
        );

        assert_eq!(map_asymptotic_to_growth_model("O(1)"), None);
        assert_eq!(map_asymptotic_to_growth_model("unknown"), None);
    }

    #[test]
    fn test_determine_verdict_unit() {
        // 1. Matched
        let (v1, _) = determine_verdict(
            Some(GrowthModel::Quadratic),
            Some(GrowthModel::Quadratic),
            false,
            Some("O(n^2)"),
        );
        assert_eq!(v1, DifferentialVerdict::Matched);

        // 2. Ambiguous (does not fail)
        let (v2, _) = determine_verdict(Some(GrowthModel::Linear), None, true, Some("O(n)"));
        assert_eq!(v2, DifferentialVerdict::Ambiguous);

        // 3. Mismatch / Deliberately wrong expected class
        let (v3, _) = determine_verdict(
            Some(GrowthModel::Linear),
            Some(GrowthModel::Quadratic),
            false,
            Some("O(n)"),
        );
        assert_eq!(v3, DifferentialVerdict::Mismatch);
    }

    #[test]
    fn test_evaluate_case_matched() {
        if compiler::discover_compiler(None).is_err() {
            return;
        }

        let cases = default_controlled_cases();
        let quad_case = cases
            .iter()
            .find(|c| c.name == "quadratic_nested_loops")
            .unwrap();
        let res = evaluate_case(quad_case);

        // Under normal conditions, quadratic matches
        if !res.ambiguous {
            assert_eq!(res.verdict, DifferentialVerdict::Matched);
            assert_eq!(res.expected_model, Some(GrowthModel::Quadratic));
            assert_eq!(res.empirical_model, Some(GrowthModel::Quadratic));
        } else {
            assert_eq!(res.verdict, DifferentialVerdict::Ambiguous);
        }
    }

    #[test]
    fn test_evaluate_case_ambiguous_does_not_fail() {
        if compiler::discover_compiler(None).is_err() {
            return;
        }

        // Program that does flat work regardless of n
        let source = r#"#include <iostream>
int main() {
    int n;
    if (std::cin >> n) {
        std::cout << 42 << "\n";
    }
    return 0;
}
"#;
        let mut vars = HashMap::new();
        vars.insert("n".to_string(), VariableBound::range(1.0, 100_000.0));
        let case = DifferentialCase {
            name: "constant_ambiguous".to_string(),
            path: "const.cpp".to_string(),
            source: source.to_string(),
            language: "cpp".to_string(),
            constraints: ConstraintSet {
                variables: vars,
                time_limit_seconds: Some(1.0),
                memory_limit_mb: Some(256.0),
                ..Default::default()
            },
            benchmark_config: BenchmarkConfig {
                sizes: vec![1_000, 2_000, 4_000, 8_000],
                repetitions: 3,
                warmup: 1,
                allow_unsandboxed: true,
                ..Default::default()
            },
            expected_class: Some("O(n)".to_string()),
        };

        let res = evaluate_case(&case);
        if res.ambiguous {
            assert_eq!(res.verdict, DifferentialVerdict::Ambiguous);
            let report = run_differential_harness(&[case]);
            assert_eq!(report.ambiguous, 1);
            assert_eq!(report.mismatched, 0);
            assert!(report.passed);
        }
    }

    #[test]
    fn test_deliberately_wrong_expected_class_fails_harness() {
        if compiler::discover_compiler(None).is_err() {
            return;
        }

        let cases = default_controlled_cases();
        let mut wrong_case = cases
            .iter()
            .find(|c| c.name == "quadratic_nested_loops")
            .unwrap()
            .clone();

        // Deliberately set expected class to linear O(n) for a quadratic program
        wrong_case.expected_class = Some("O(n)".to_string());

        let res = evaluate_case(&wrong_case);
        if !res.ambiguous {
            assert_eq!(res.verdict, DifferentialVerdict::Mismatch);
            assert_eq!(res.expected_model, Some(GrowthModel::Linear));
            assert_ne!(res.empirical_model, Some(GrowthModel::Linear));

            let report = run_differential_harness(&[wrong_case]);
            assert_eq!(report.mismatched, 1);
            // Crucial: deliberate mismatch MUST fail the harness
            assert!(!report.passed);
        }
    }

    #[test]
    fn test_report_serialization() {
        let report = DifferentialReport {
            total: 2,
            matched: 1,
            ambiguous: 1,
            mismatched: 0,
            errors: 0,
            passed: true,
            cases: vec![
                CaseResult {
                    name: "case1".to_string(),
                    path: "case1.cpp".to_string(),
                    static_asymptotic: Some("O(n)".to_string()),
                    expected_model: Some(GrowthModel::Linear),
                    empirical_model: Some(GrowthModel::Linear),
                    ambiguous: false,
                    r_squared: Some(0.99),
                    rss: Some(0.01),
                    verdict: DifferentialVerdict::Matched,
                    message: "matched".to_string(),
                },
                CaseResult {
                    name: "case2".to_string(),
                    path: "case2.cpp".to_string(),
                    static_asymptotic: Some("O(n log n)".to_string()),
                    expected_model: Some(GrowthModel::Linearithmic),
                    empirical_model: None,
                    ambiguous: true,
                    r_squared: Some(0.8),
                    rss: Some(0.2),
                    verdict: DifferentialVerdict::Ambiguous,
                    message: "ambiguous".to_string(),
                },
            ],
        };

        let temp_dir = std::env::temp_dir().join(format!("diff_test_{}", std::process::id()));
        let report_path = temp_dir.join("results.json");

        assert!(write_report(&report, &report_path).is_ok());
        assert!(report_path.exists());

        let read_back: DifferentialReport =
            serde_json::from_str(&fs::read_to_string(&report_path).unwrap()).unwrap();
        assert_eq!(read_back.total, 2);
        assert_eq!(read_back.matched, 1);
        assert_eq!(read_back.ambiguous, 1);
        assert!(read_back.passed);

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
