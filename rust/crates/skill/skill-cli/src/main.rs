fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let output = skill_cli::execute(&args, &mut skill_cli::read_stdin);
    print!("{}", output.stdout);
    eprint!("{}", output.stderr);
    std::process::ExitCode::from(output.code)
}
