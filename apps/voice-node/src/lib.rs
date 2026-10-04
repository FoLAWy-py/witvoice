#![forbid(unsafe_code)]

#[cfg(windows)]
pub fn run(args: impl Iterator<Item = String>) -> std::io::Result<()> {
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
            }
            Err(_) => server.disconnect(),
        }
    }
}
