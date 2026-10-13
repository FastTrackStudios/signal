//! Parse → write → re-parse round-trip integrity check, for one session.
//!
//! The check itself is [`dawfile_protools::check_round_trip`]; this is the
//! command-line way to point it at a file.
//!
//! Usage:
//!   cargo run -p dawfile-protools --example round_trip <path>

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        eprintln!("usage: {} <path-to-session>", args[0]);
        return ExitCode::from(2);
    }
    let path = PathBuf::from(&args[1]);

    match dawfile_protools::check_round_trip(&path) {
        Ok(()) => {
            println!("round-trip OK: {}", path.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("round-trip FAIL: {}: {e}", path.display());
            ExitCode::FAILURE
        }
    }
}
