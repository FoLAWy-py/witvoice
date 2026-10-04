//! Development-only native process harness, guarded by required-features.
#![forbid(unsafe_code)]
#![cfg_attr(windows, windows_subsystem = "windows")]
#[cfg(windows)]
fn main() -> std::io::Result<()> {
    use std::os::windows::process::CommandExt;
    use std::{
        io::Write,
        path::PathBuf,
        process::{Command, Stdio},
        time::Duration,
    };
    use witvoice_platform::{ProcessJob, Secret, launch_node_from_ui_job};
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--endpoint") => witvoice_node::run_with_process_probe(args.into_iter()),
        Some("argv-check") => {
            println!("{}", serde_json::to_string(&args[1..])?);
            Ok(())
        }
        Some("leaf") if args.len() == 1 => loop {
            std::thread::sleep(Duration::from_secs(1));
        },
        Some("worker-tree") if args.len() == 1 => {
            let child = Command::new(std::env::current_exe()?)
                .creation_flags(0x08000000) // CREATE_NO_WINDOW: no console helper in the tree.
                .arg("leaf")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?;
            println!("{}", child.id());
            std::io::stdout().flush()?;
            loop {
                std::thread::sleep(Duration::from_secs(1));
            }
        }
        Some("ui-parent") if args.len() == 3 => {
            let mut token = [0; 32];
            std::io::Read::read_exact(&mut std::io::stdin(), &mut token)?;
            let program: PathBuf = std::env::current_exe()?.with_file_name("witvoice-node.exe");
            let ui = ProcessJob::open_ui_query(&args[2])?;
            let launched =
                launch_node_from_ui_job(&program, &args[1], &Secret::from_bytes(token), &ui);
            drop(ui); // Root retains the only job owner before KillOnClose is exercised.
            match launched {
                Ok(node) => {
                    println!("node:{}", node.id());
                    std::io::stdout().flush()?;
                    loop {
                        std::thread::sleep(Duration::from_secs(1));
                    }
                }
                Err(error) => {
                    println!("denied:{:?}", error.kind());
                    std::io::stdout().flush()?;
                    Err(error)
                }
            }
        }
        _ => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "invalid fixed probe mode",
        )),
    }
}
#[cfg(not(windows))]
fn main() {
    std::process::exit(1);
}
