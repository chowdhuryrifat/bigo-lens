//! Differential test harness binary.
//!
//! Executes controlled fixture programs, comparing static complexity predictions
//! against empirical growth fitting.
//! Conforms to BL-408 and outputs a structured JSON report.

use bigolens_engine::differential::{
    default_controlled_cases, run_differential_harness, write_report,
};
use clap::Parser;
use std::path::PathBuf;
use std::process;

#[derive(Parser, Debug)]
#[command(name = "differential-harness")]
#[command(
    about = "Differential test harness comparing static analysis with empirical benchmark fits"
)]
struct Cli {
    /// Path to output the structured JSON report artifact
    #[arg(long, default_value = "target/differential-results.json")]
    output: PathBuf,

    /// Enable verbose diagnostic output
    #[arg(short, long)]
    verbose: bool,
}

fn main() {
    let cli = Cli::parse();

    println!("============================================================");
    println!("        BigO Lens Differential Test Harness (BL-408)        ");
    println!("============================================================");

    let cases = default_controlled_cases();
    println!("Evaluating {} controlled test case(s)...\n", cases.len());

    let report = run_differential_harness(&cases);

    for case_res in &report.cases {
        println!(
            "[{}] {} -> expected: {:?}, empirical: {:?}, ambiguous: {}\n    Note: {}",
            case_res.verdict,
            case_res.name,
            case_res.expected_model,
            case_res.empirical_model,
            case_res.ambiguous,
            case_res.message
        );
    }

    println!("\n------------------------------------------------------------");
    println!("Summary:");
    println!("  Total cases:  {}", report.total);
    println!("  Matched:      {}", report.matched);
    println!("  Ambiguous:    {}", report.ambiguous);
    println!("  Mismatched:   {}", report.mismatched);
    println!("  Errors:       {}", report.errors);
    println!(
        "  Harness Status: {}",
        if report.passed { "PASSED" } else { "FAILED" }
    );
    println!("------------------------------------------------------------");

    if let Err(e) = write_report(&report, &cli.output) {
        eprintln!(
            "differential-harness: failed to write results report to {:?}: {}",
            cli.output, e
        );
        process::exit(1);
    }
    println!("Results artifact written to: {:?}", cli.output);

    if report.passed {
        println!("Differential harness completed successfully.");
        process::exit(0);
    } else {
        eprintln!(
            "differential-harness: FAILED (mismatches: {}, errors: {})",
            report.mismatched, report.errors
        );
        process::exit(1);
    }
}
