#![forbid(unsafe_code)]

#[cfg(windows)]
pub fn run(args: impl Iterator<Item = String>) -> std::io::Result<()> {
    run_inner(args, false)
}

/// Feature-gated real-process harness, never a selectable engine or a model.
#[cfg(all(windows, feature = "process-tests"))]
pub fn run_with_process_probe(args: impl Iterator<Item = String>) -> std::io::Result<()> {
    run_inner(args, true)
}

/// Nonblocking trusted supervisor seam used only by ordinary-process tests.
/// The same Runtime retires output before the caller performs Job cleanup.
#[cfg(all(windows, feature = "process-tests"))]
pub fn retire_failed_test_worker(
    runtime: &mut witvoice_session::Runtime,
    worker: &witvoice_platform::Process,
) -> std::io::Result<bool> {
    if worker.wait(std::time::Duration::ZERO)?.is_none() {
        return Ok(false);
    }
    let binding = runtime
        .test_binding()
        .cloned()
        .ok_or_else(|| std::io::Error::other("no trusted fixture binding"))?;
    runtime
        .test_worker_failed(&binding)
        .map_err(|code| std::io::Error::other(format!("trusted worker failure: {code:?}")))?;
    Ok(true)
}

#[cfg(windows)]
fn run_inner(args: impl Iterator<Item = String>, probe_worker: bool) -> std::io::Result<()> {
    use std::{io::Read, sync::mpsc, time::Duration};
    use witvoice_platform::{PipeServer, Secret, current_process_is_elevated};
    use witvoice_session::Runtime;

    let args: Vec<_> = args.collect();
    if args.len() != 3 || args[0] != "--endpoint" || args[2] != "--bootstrap-stdin" {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "invalid Node arguments",
        ));
    }
    if current_process_is_elevated()? {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "Node requires a non-elevated user process",
        ));
    }
    // A launcher supplies a CSPRNG token through a private inherited stdin pipe.
    // No token is accepted via a command line, environment variable or log.
    let (sender, receiver) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let mut bytes = [0; 32];
        let result = std::io::stdin()
            .read_exact(&mut bytes)
            .map(|()| Secret::from_bytes(bytes));
        let _ = sender.send(result);
    });
    let secret = receiver
        .recv_timeout(Duration::from_secs(3))
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "bootstrap deadline"))??;
    let mut server = PipeServer::bind(&args[1], secret)?;
    let mut runtime = Runtime::default();
    let mut worker: Option<(witvoice_platform::ProcessJob, witvoice_platform::Process)> = None;
    #[cfg(feature = "process-tests")]
    if probe_worker {
        use std::io::{BufRead, BufReader, Write};
        let job = witvoice_platform::ProcessJob::new(false)?;
        // Fixed own executable and argv; no wire request can supply either.
        let mut process = job.spawn(&std::env::current_exe()?, &["worker-tree".into()], true)?;
        let output = process
            .take_output()
            .ok_or_else(|| std::io::Error::other("missing probe output"))?;
        let (sender, receiver) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let mut line = String::new();
            let result = BufReader::new(output.take(64))
                .read_line(&mut line)
                .map(|_| line);
            let _ = sender.send(result);
        });
        let child = receiver
            .recv_timeout(Duration::from_secs(3))
            .map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::TimedOut, "probe startup deadline")
            })??;
        let child: u32 = child
            .trim()
            .parse()
            .map_err(|_| std::io::Error::other("invalid probe report"))?;
        println!("worker:{}:{child}", process.id());
        std::io::stdout().flush()?;
        worker = Some((job, process));
    }
    #[cfg(not(feature = "process-tests"))]
    let _ = probe_worker;
    // A UI disconnect neither drops this owner nor signals a Node shutdown.
    loop {
        match server.receive(Duration::from_secs(3)) {
            Ok(payload) => {
                if server.connection_is_current()
                    && let Ok(response) = runtime.handle(&payload)
                    && let Ok(bytes) = serde_json::to_vec(&response)
                {
                    let _ = server.respond(&bytes);
                }
                server.disconnect();
                if runtime.take_resource_cleanup()
                    && let Some((job, _)) = worker.take()
                {
                    job.terminate()?;
                }
                if runtime.shutdown_requested() {
                    return Ok(());
                }
            }
            Err(_) => server.disconnect(),
        }
    }
}
