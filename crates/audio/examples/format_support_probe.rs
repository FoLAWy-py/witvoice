//! Metadata only: no stream Initialize/Start, capture/render or default UID.
mod support;
#[cfg(windows)]
fn main() -> Result<(), String> {
    use witvoice_audio::{
        format::{AudioFormat, Encoding},
        wasapi::probe_format,
    };
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if !(2..=4).contains(&args.len()) {
        return Err(
            "PRIVATE_SELECTION_JSON RATE:CHANNELS:ENCODING [up to 3 explicit candidates]".into(),
        );
    }
    let (uid, flow) = support::selection(&args[0])?;
    let candidates = args[1..]
        .iter()
        .map(|arg| {
            let text = arg.to_str().ok_or("candidate must be UTF8")?;
            let fields: Vec<_> = text.split(':').collect();
            if fields.len() != 3 {
                return Err("candidate must be rate:channels:encoding".to_string());
            }
            let encoding = match fields[2] {
                "pcm8" => Encoding::Pcm8,
                "pcm16" => Encoding::Pcm16,
                "pcm24" => Encoding::Pcm24,
                "pcm32" => Encoding::Pcm32,
                "float32" => Encoding::Float32,
                _ => return Err("invalid encoding".into()),
            };
            AudioFormat::new(
                fields[0].parse().map_err(|_| "invalid rate")?,
                fields[1].parse().map_err(|_| "invalid channels")?,
                encoding,
            )
            .map_err(|_| "invalid candidate format".to_string())
        })
        .collect::<Result<Vec<_>, String>>()?;
    // Validate every explicit candidate before touching COM. No fallback loop.
    let mut results = Vec::new();
    let mut rejected = false;
    for candidate in candidates {
        match probe_format(&uid, flow, candidate) {
            Ok(result) => {
                rejected |= !result.exact_supported;
                results.push(serde_json::json!({"probe": result}));
            }
            Err(error) => {
                rejected |= matches!(
                    error,
                    witvoice_audio::wasapi::MetadataError::Com {
                        operation: "IsFormatSupported",
                        ..
                    }
                );
                results.push(serde_json::json!({"requested":candidate, "error":error}));
            }
        }
    }
    println!(
        "{}",
        serde_json::json!({"results": results, "initialize_start": "NOT_RUN",
        "exact_rejection": if rejected {"OBSERVED"} else {"NOT_RUN"}})
    );
    Ok(())
}
#[cfg(not(windows))]
fn main() {
    eprintln!("Windows only; NOT_RUN");
    std::process::exit(2);
}
