//! BigO Lens standalone engine binary.
//!
//! Provides the command-line interface and executes the versioned JSON protocol over stdin/stdout.

use bigolens_core::api::MAX_REQUEST_BYTES;
use bigolens_engine::handle_request;
use clap::Parser;
use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;
use std::process;

#[derive(Parser, Debug)]
#[command(name = "bigolens-engine")]
#[command(version = bigolens_core::api::ENGINE_VERSION)]
#[command(about = "BigO Lens static complexity and workload analyzer")]
struct Cli {
    /// Read JSON request from a file instead of standard input
    #[arg(long, value_name = "FILE")]
    request: Option<PathBuf>,
}

fn main() {
    let cli = Cli::parse();

    let input = match cli.request {
        Some(path) => match fs::read_to_string(&path) {
            Ok(content) => content,
            Err(e) => {
                eprintln!(
                    "bigolens-engine: failed to read request file '{:?}': {}",
                    path, e
                );
                process::exit(1);
            }
        },
        None => {
            let mut buffer = String::new();
            // Read up to 4 MiB + 1 byte to detect overflow
            let mut handle = io::stdin().take((MAX_REQUEST_BYTES + 1) as u64);
            if let Err(e) = handle.read_to_string(&mut buffer) {
                eprintln!("bigolens-engine: failed to read from stdin: {}", e);
                process::exit(1);
            }
            buffer
        }
    };

    let response = handle_request(&input);

    // Reserved stdout: exactly one line
    println!("{}", response);
}
