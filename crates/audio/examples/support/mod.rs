#[cfg(windows)]
pub fn selection(path: &std::ffi::OsStr) -> Result<(String, witvoice_audio::wasapi::Flow), String> {
    use std::{io::Read, path::PathBuf};
    #[derive(serde::Deserialize)]
    struct Selection {
        uid: String,
        flow: String,
    }
    let path = PathBuf::from(path);
    if !path.is_absolute() {
        return Err("explicit absolute private selection required".into());
    }
    let mut bytes = Vec::with_capacity(8193);
    std::fs::File::open(path)
        .map_err(|_| "selection unavailable")?
        .take(8193)
        .read_to_end(&mut bytes)
        .map_err(|_| "selection read failed")?;
    if bytes.len() > 8192 {
        return Err("selection exceeds 8192 bytes".into());
    }
    // Existing Endpoint metadata has other fields; only exact UID/flow is used.
    let selected: Selection =
        serde_json::from_slice(&bytes).map_err(|_| "invalid selection JSON")?;
    if selected.uid.is_empty() || selected.uid.len() > 1024 || selected.uid.contains('\0') {
        return Err("invalid explicit UID".into());
    }
    let flow = match selected.flow.as_str() {
        "Capture" => witvoice_audio::wasapi::Flow::Capture,
        "Render" => witvoice_audio::wasapi::Flow::Render,
        _ => return Err("explicit Capture or Render flow required".into()),
    };
    Ok((selected.uid, flow))
}
