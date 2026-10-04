//! Developer process probe. No device/model/audio access.
#[cfg(windows)]
fn main() -> std::io::Result<()> {
    use std::{
        io::{Read, Write},
        time::Duration,
    };
    use witvoice_platform::{PipeClient, Secret};
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 3 {
        return Err(std::io::Error::other("invalid probe arguments"));
    }
    let pid = args[1]
        .parse()
        .map_err(|_| std::io::Error::other("invalid process ID"))?;
    let mut bytes = [0; 32];
    std::io::stdin().read_exact(&mut bytes)?;
    let secret = Secret::from_bytes(bytes);
    let mut client = PipeClient::connect(&args[0], pid, Duration::from_secs(3))?;
    client.authenticate(&secret)?;
    match args[2].as_str() {
        "hold-partial" => {
            client.write_fragment(&[0])?;
            println!("partial-open");
            std::io::stdout().flush()?;
            std::thread::sleep(Duration::from_secs(30));
        }
        "state" => {
            let response = client.request(br#"{"protocol_version":1,"request_id":"00000000-0000-0000-0000-000000000001","command":{"kind":"GetState"}}"#)?;
            std::io::stdout().write_all(&response)?;
        }
        _ => return Err(std::io::Error::other("invalid probe mode")),
    }
    Ok(())
}

#[cfg(not(windows))]
fn main() {
    std::process::exit(1);
}
