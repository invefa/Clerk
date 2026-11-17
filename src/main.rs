//! `clerk`: manager for managed design documents in a versioned workspace.

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(clerk::interface::cli::run(argv));
}
