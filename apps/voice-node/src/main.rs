#[cfg(windows)]
fn main() {
    if witvoice_node::run(std::env::args().skip(1)).is_err() {
        // Do not print payloads, tokens, user paths or Windows account identifiers.
        eprintln!("Node control startup or transport failed");
        std::process::exit(1);
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("This Node slice requires Windows; Mac runtime is untested");
    std::process::exit(1);
}
