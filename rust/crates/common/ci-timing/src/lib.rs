//! Opt-in timing diagnostics shared by workspace crates.

use std::sync::{Mutex, OnceLock};
use std::time::Instant;

static FILE_READ_TIMING: OnceLock<Mutex<(u64, u128)>> = OnceLock::new();
static CLI_TIMING_ENABLED: OnceLock<bool> = OnceLock::new();

fn cli_timing_enabled() -> bool {
    *CLI_TIMING_ENABLED.get_or_init(|| std::env::var_os("AWK_CLI_TIMING").is_some())
}

pub fn test_timing_enabled() -> bool {
    std::env::var_os("AWK_TEST_TIMING").is_some()
}

pub fn log_test_timing(label: &str, started: Instant) {
    if test_timing_enabled() {
        eprintln!("AWK_TEST_TIMING {label} {}", started.elapsed().as_micros());
    }
}

pub fn log_cli_timing(label: &str, started: Instant) {
    if cli_timing_enabled() {
        let micros = started.elapsed().as_micros();
        if label == "artifact_file_read" {
            let mut timing = FILE_READ_TIMING
                .get_or_init(|| Mutex::new((0, 0)))
                .lock()
                .expect("timing lock");
            timing.0 += 1;
            timing.1 += micros;
        } else {
            eprintln!("AWK_CLI_TIMING {label} {micros}");
        }
    }
}

pub fn flush_cli_timings() {
    if cli_timing_enabled()
        && let Some(timing) = FILE_READ_TIMING.get()
    {
        let (count, micros) = *timing.lock().expect("timing lock");
        eprintln!("AWK_CLI_TIMING_AGG artifact_file_read {count} {micros}");
    }
}
