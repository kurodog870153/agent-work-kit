use std::io::Write;

fn main() {
    let started = std::time::Instant::now();
    let tokens: Vec<String> = std::env::args().skip(1).collect();
    let (exit_code, result) = work_cli::runtime::run(&tokens);
    ci_timing::log_cli_timing("total", started);
    ci_timing::flush_cli_timings();
    let bytes = result.to_stdout().expect("CLI response serializes");
    if std::io::stdout().write_all(&bytes).is_err() {
        std::process::exit(10);
    }
    std::process::exit(exit_code);
}
