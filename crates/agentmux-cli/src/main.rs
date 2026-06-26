fn main() {
    if let Err(error) =
        agentmux_cli::run_cli_with_program("agentmux", std::env::args().skip(1), std::io::stdout())
    {
        eprintln!("agentmux: {error}");
        std::process::exit(1);
    }
}
