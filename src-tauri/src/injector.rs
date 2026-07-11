

use crate::mono;
use serde::Deserialize;
use std::io::{BufRead, BufReader};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Duration;
use tauri::{AppHandle, Emitter};


const LOG_EVENT: &str = "zyringe://log";

#[derive(Debug, Deserialize)]
pub struct InjectRequest {
    pub pid: i32,
    pub dll_path: String,
    pub namespace: String,
    pub class_name: String,
    pub method: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub timeout_secs: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct LaunchRequest {
    pub exe_path: String,
    pub dll_path: String,
    pub namespace: String,
    pub class_name: String,
    pub method: String,

    pub working_dir: Option<String>,
}

fn log(app: &AppHandle, line: impl Into<String>) {
    let _ = app.emit(LOG_EVENT, line.into());
}


pub fn cancel_flag_path(pid: i32) -> PathBuf {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
        .unwrap_or_else(std::env::temp_dir);
    dir.join(format!("zyringe-cancel-{pid}"))
}


pub fn cancel(pid: i32) -> Result<(), String> {
    let path = cancel_flag_path(pid);
    std::fs::write(&path, b"1").map_err(|e| format!("could not raise cancel flag: {e}"))
}

fn validate_dll(path: &str) -> Result<(), String> {
    let p = PathBuf::from(path);
    if !p.is_file() {
        return Err(format!("DLL not found: {path}"));
    }
    Ok(())
}


pub fn inject(app: &AppHandle, req: InjectRequest) -> Result<(), String> {
    validate_dll(&req.dll_path)?;
    let helper = mono::helper_so(app)?;
    let bin = mono::inject_bin(app)?;

    log(app, format!("→ Preparing ptrace injection into PID {}", req.pid));
    log(app, format!("  helper: {}", helper.display()));
    log(app, "  requesting elevation via pkexec…");


    let cancel_flag = cancel_flag_path(req.pid);
    let _ = std::fs::remove_file(&cancel_flag);

    let mut cmd = Command::new("pkexec");
    cmd.arg(&bin)
        .arg("--pid")
        .arg(req.pid.to_string())
        .arg("--cancel-flag")
        .arg(&cancel_flag)
        .arg("--so")
        .arg(&helper)
        .arg("--dll")
        .arg(&req.dll_path)
        .arg("--namespace")
        .arg(&req.namespace)
        .arg("--class")
        .arg(&req.class_name)
        .arg("--method")
        .arg(&req.method);
    for a in &req.args {
        cmd.arg("--arg").arg(a);
    }
    if let Some(secs) = req.timeout_secs {
        cmd.arg("--timeout").arg(secs.to_string());
    }

    run_streaming(app, cmd)
}


pub fn launch_with_preload(app: &AppHandle, req: LaunchRequest) -> Result<(), String> {
    validate_dll(&req.dll_path)?;
    let helper = mono::helper_so(app)?;
    let exe = PathBuf::from(&req.exe_path);
    if !exe.is_file() {
        return Err(format!("Executable not found: {}", req.exe_path));
    }
    let workdir = req
        .working_dir
        .clone()
        .map(PathBuf::from)
        .or_else(|| exe.parent().map(|p| p.to_path_buf()))
        .ok_or("Could not determine working directory")?;


    let config_path = write_launch_config(&req)?;
    log(app, format!("→ Launching {} with LD_PRELOAD", req.exe_path));
    log(app, format!("  helper: {}", helper.display()));
    log(app, format!("  config: {}", config_path.display()));

    let mut cmd = Command::new(&exe);
    cmd.current_dir(&workdir)
        .env("LD_PRELOAD", &helper)
        .env("ZYRINGE_CONFIG", &config_path);

    run_launch(app, cmd)
}


fn run_launch(app: &AppHandle, mut cmd: Command) -> Result<(), String> {
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| format!("spawn failed: {e}"))?;
    let pid = child.id();

    let (tx, rx) = mpsc::channel::<Result<(), String>>();
    let done = Arc::new(AtomicBool::new(false));

    fn scan(line: &str) -> Option<Result<(), String>> {
        if line.contains("invoked") && line.contains(" OK") {
            return Some(Ok(()));
        }
        line.find("preload:")
            .map(|i| Err(line[i + "preload:".len()..].trim().to_string()))
    }

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();


    if let Some(s) = stderr {
        let (app, tx, done) = (app.clone(), tx.clone(), done.clone());
        std::thread::spawn(move || {
            for line in BufReader::new(s).lines().map_while(Result::ok) {
                if !done.load(Ordering::Relaxed) {
                    log(&app, format!("  {line}"));
                }
                if let Some(r) = scan(&line) {
                    done.store(true, Ordering::Relaxed);
                    let _ = tx.send(r);
                }
            }
        });
    }

    {
        let (app, tx, done) = (app.clone(), tx.clone(), done.clone());
        std::thread::spawn(move || {
            if let Some(s) = stdout {
                for line in BufReader::new(s).lines().map_while(Result::ok) {
                    if !done.load(Ordering::Relaxed) {
                        log(&app, format!("  {line}"));
                    }
                    if let Some(r) = scan(&line) {
                        done.store(true, Ordering::Relaxed);
                        let _ = tx.send(r);
                    }
                }
            }
            let _ = child.wait();
        });
    }


    {
        let (tx, done) = (tx.clone(), done.clone());
        std::thread::spawn(move || {
            let path = format!("/tmp/.zyringe/helper-{pid}.log");
            let mut seen = 0usize;
            for _ in 0..190 {
                if done.load(Ordering::Relaxed) {
                    return;
                }
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if content.len() > seen {
                        let fresh = content[seen..].to_string();
                        seen = content.len();
                        for line in fresh.lines() {
                            if let Some(r) = scan(line) {
                                done.store(true, Ordering::Relaxed);
                                let _ = tx.send(r);
                                return;
                            }
                        }
                    }
                }
                std::thread::sleep(Duration::from_millis(500));
            }
        });
    }
    drop(tx);

    match rx.recv_timeout(Duration::from_secs(90)) {
        Ok(Ok(())) => {
            log(app, "✓ Mod loaded — entry point invoked. The game keeps running.");
            Ok(())
        }
        Ok(Err(e)) => Err(format!("Mod failed to load — {e}")),
        Err(mpsc::RecvTimeoutError::Timeout) => Err(
            "Game launched, but the mod hasn't reported back yet (still loading?). Check the Helper log."
                .into(),
        ),
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(
            "Game exited before the mod loaded, or it produced no result. Check the Helper log.".into(),
        ),
    }
}


fn write_launch_config(req: &LaunchRequest) -> Result<PathBuf, String> {
    let dir = PathBuf::from("/tmp/.zyringe");
    std::fs::create_dir_all(&dir).map_err(|e| format!("mkdir /tmp/.zyringe: {e}"))?;
    let path = dir.join(format!("launch-{}.json", std::process::id()));
    let json = serde_json::json!({
        "dll": req.dll_path,
        "namespace": req.namespace,
        "class": req.class_name,
        "method": req.method,
    });
    std::fs::write(&path, serde_json::to_vec(&json).unwrap())
        .map_err(|e| format!("write config: {e}"))?;
    let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    Ok(path)
}


fn run_streaming(app: &AppHandle, mut cmd: Command) -> Result<(), String> {
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| format!("spawn failed: {e}"))?;


    if let Some(stderr) = child.stderr.take() {
        let app = app.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                log(&app, format!("  {line}"));
            }
        });
    }

    if let Some(stdout) = child.stdout.take() {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            log(app, format!("  {line}"));
        }
    }

    let status = child.wait().map_err(|e| format!("wait failed: {e}"))?;
    match status.code() {
        Some(0) => {
            log(app, "✓ Injection succeeded.");
            Ok(())
        }
        Some(126) | Some(127) => {

            Err("Elevation was cancelled or not authorized (pkexec).".into())
        }
        Some(code) => Err(format!("Injection failed (exit code {code}).")),
        None => Err("Injection process terminated by signal.".into()),
    }
}
