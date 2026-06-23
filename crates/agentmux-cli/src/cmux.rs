fn main() {
    if let Err(error) =
        agentmux_cli::run_cli_with_program("cmux", std::env::args().skip(1), std::io::stdout())
    {
        eprintln!("cmux: {error}");
        std::process::exit(1);
    }
}
