use std::io::Write;

fn main() {
    let tokens: Vec<String> = std::env::args().skip(1).collect();
    let (code, result) = tooling_cli::run(&tokens);
    let bytes = serde_json::to_vec_pretty(&result).expect("CLI result serializes");
    let mut stdout = std::io::stdout();
    if stdout.write_all(&bytes).is_err() || stdout.write_all(b"\n").is_err() {
        std::process::exit(10);
    }
    std::process::exit(code);
}
