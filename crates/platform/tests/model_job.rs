#![cfg(windows)]
//! Software process test only: no model, Torch, PCM, audio or network.
use std::{ffi::OsString, io::Read, path::Path, time::Duration};
use witvoice_platform::{ProcessJob, current_process_is_elevated};

#[test]
fn python_model_style_descendant_stays_in_owned_job_after_controller_exit() {
    assert!(!current_process_is_elevated().unwrap());
    let python = Path::new(
        r"C:\Users\22198\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe",
    );
    assert!(python.is_file(), "fixed native interpreter prerequisite");
    let job = ProcessJob::new(false).unwrap();
    // Finite child even if this test fails. close_fds and NUL streams match the
    // production model owner; no breakaway flag or inherited output writer.
    let script = "import subprocess,sys; p=subprocess.Popen([sys.executable,'-I','-S','-c','import time;time.sleep(10)'],stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,close_fds=True,creationflags=subprocess.CREATE_NO_WINDOW);print(p.pid,flush=True)";
    let args: Vec<OsString> = ["-I", "-S", "-c", script]
        .into_iter()
        .map(Into::into)
        .collect();
    let mut controller = job.spawn(python, &args, true).unwrap();
    assert_eq!(controller.wait(Duration::from_secs(3)).unwrap(), Some(0));
    let mut output = Vec::with_capacity(33);
    controller
        .take_output()
        .unwrap()
        .take(33)
        .read_to_end(&mut output)
        .unwrap();
    assert!(output.len() <= 32);
    let model_pid: u32 = std::str::from_utf8(&output)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert_ne!(model_pid, controller.id());
    assert!(job.process_ids().unwrap().contains(&model_pid));
    assert!(job.active_processes().unwrap() >= 1);
    job.terminate().unwrap();
    assert_eq!(job.active_processes().unwrap(), 0);
    assert!(job.process_ids().unwrap().is_empty());
}
