//! Requires explicit operator permission and an exclusive audio hardware lease.
//! Never run as a cargo test. No PCM files, upstream upload, or default selection.
#[cfg(windows)]
fn main() -> Result<(), String> {
    use std::time::{Duration, Instant};
    use witvoice_audio::{
        format::{AudioFormat, Encoding},
        stream::{ExplicitStart, SharedStream},
        wasapi::Flow,
    };
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 7 || args[6] != "--approve-start" {
        return Err(
            "capture|silence-render UID rate channels pcm16|float32 seconds(1..5) --approve-start"
                .into(),
        );
    }
    let flow = match args[0].as_str() {
        "capture" => Flow::Capture,
        "silence-render" => Flow::Render,
        _ => return Err("explicit flow required".into()),
    };
    let rate: u32 = args[2].parse().map_err(|_| "invalid rate")?;
    let channels = args[3].parse().map_err(|_| "invalid channels")?;
    let encoding = match args[4].as_str() {
        "pcm16" => Encoding::Pcm16,
        "float32" => Encoding::Float32,
        _ => return Err("explicit encoding required".into()),
    };
    let seconds: u64 = args[5].parse().map_err(|_| "invalid duration")?;
    if !(1..=5).contains(&seconds) {
        return Err("maximum five seconds".into());
    }
    let format = AudioFormat::new(rate, channels, encoding).map_err(|e| format!("{e:?}"))?;
    let mut owner =
        SharedStream::prepare(&args[1], flow, format, rate / 5).map_err(|e| format!("{e:?}"))?;
    let mut output = vec![0.0; owner.capacity_frames() as usize];
    let mut packets = 0u64;
    let mut frames = 0u64;
    let started = Instant::now();
    owner
        .start(ExplicitStart::UserApproved)
        .map_err(|e| format!("{e:?}"))?;
    let requested = Duration::from_secs(seconds);
    while let Some(remaining) = requested.checked_sub(started.elapsed()) {
        if remaining.is_zero() {
            break;
        }
        let ready = owner
            .wait_event(remaining.min(Duration::from_millis(50)))
            .map_err(|e| format!("{e:?}"))?;
        if started.elapsed() >= requested {
            break;
        }
        if !ready {
            continue;
        }
        if flow == Flow::Capture {
            if let Some(packet) = owner
                .capture_packet(&mut output)
                .map_err(|e| format!("{e:?}"))?
            {
                packets += 1;
                frames += u64::from(packet.frames);
            }
            output.fill(0.0);
        } else {
            frames += u64::from(owner.submit_silence().map_err(|e| format!("{e:?}"))?);
        }
    }
    let active_wall = started.elapsed();
    owner.close().map_err(|e| format!("{e:?}"))?;
    println!(
        "native-rate only; packets={packets} frames={frames} request_seconds={seconds} active_wall_seconds={:.6} wall_including_close_seconds={:.6}; no PCM saved; not full T007 acceptance",
        active_wall.as_secs_f64(),
        started.elapsed().as_secs_f64()
    );
    Ok(())
}
#[cfg(not(windows))]
fn main() {
    eprintln!("Windows only; Mac NOT_RUN");
    std::process::exit(1);
}
