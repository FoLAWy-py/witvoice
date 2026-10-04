//! Metadata-only diagnostic. Redirect to .local; endpoint IDs are private.
#[cfg(windows)]
fn main() -> Result<(), String> {
    let endpoints = witvoice_audio::wasapi::enumerate_endpoints().map_err(|e| format!("{e:?}"))?;
    let text = serde_json::to_string_pretty(&endpoints).map_err(|e| e.to_string())?;
    println!("{text}");
    Ok(())
}

#[cfg(not(windows))]
fn main() {
    eprintln!("WASAPI metadata is available only on Windows; no Mac backend is implemented.");
    std::process::exit(2);
}
