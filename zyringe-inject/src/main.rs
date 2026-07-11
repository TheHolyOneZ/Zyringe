

mod elf;
mod maps;
mod ptrace_engine;

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::exit;

struct Args {
    pid: i32,
    so: String,
    dll: String,
    namespace: String,
    class: String,
    method: String,
    args: Vec<String>,
    timeout_secs: Option<u32>,
    cancel_flag: Option<String>,
}

fn parse_args() -> Result<Args, String> {
    let mut pid = None;
    let mut so = None;
    let mut dll = None;
    let mut namespace = String::new();
    let mut class = None;
    let mut method = None;
    let mut args: Vec<String> = Vec::new();
    let mut timeout_secs: Option<u32> = None;
    let mut cancel_flag: Option<String> = None;

    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < argv.len() {
        let flag = argv[i].as_str();
        let take_val = |i: &mut usize| -> Result<String, String> {
            *i += 1;
            argv.get(*i)
                .cloned()
                .ok_or_else(|| format!("missing value for {flag}"))
        };
        match flag {
            "--pid" => {
                pid = Some(take_val(&mut i)?.parse::<i32>().map_err(|_| "bad --pid".to_string())?)
            }
            "--so" => so = Some(take_val(&mut i)?),
            "--dll" => dll = Some(take_val(&mut i)?),
            "--namespace" => namespace = take_val(&mut i)?,
            "--class" => class = Some(take_val(&mut i)?),
            "--method" => method = Some(take_val(&mut i)?),
            "--arg" => args.push(take_val(&mut i)?),
            "--timeout" => {
                timeout_secs = Some(take_val(&mut i)?.parse::<u32>().map_err(|_| "bad --timeout".to_string())?)
            }
            "--cancel-flag" => cancel_flag = Some(take_val(&mut i)?),
            "-h" | "--help" => {
                println!("usage: zyringe-inject --pid N --so SO --dll DLL --namespace NS --class C --method M");
                exit(0);
            }
            other => return Err(format!("unknown argument: {other}")),
        }
        i += 1;
    }

    Ok(Args {
        pid: pid.ok_or("--pid is required")?,
        so: so.ok_or("--so is required")?,
        dll: dll.ok_or("--dll is required")?,
        namespace,
        class: class.ok_or("--class is required")?,
        method: method.ok_or("--method is required")?,
        args,
        timeout_secs,
        cancel_flag,
    })
}


fn elf_class(pid: i32) -> Result<u8, String> {
    use std::io::Read;
    let mut f = std::fs::File::open(format!("/proc/{pid}/exe"))
        .map_err(|e| format!("open exe: {e}"))?;
    let mut hdr = [0u8; 5];
    f.read_exact(&mut hdr).map_err(|e| format!("read exe: {e}"))?;
    if &hdr[0..4] != b"\x7fELF" {
        return Err("target is not an ELF binary".into());
    }
    Ok(hdr[4])
}

fn run() -> Result<(), String> {
    let args = parse_args()?;

    if !PathBuf::from(&args.so).is_file() {
        return Err(format!("helper .so not found: {}", args.so));
    }
    if !PathBuf::from(format!("/proc/{}", args.pid)).is_dir() {
        return Err(format!("no such process: {}", args.pid));
    }


    match elf_class(args.pid) {
        Ok(2) => {}
        Ok(1) => return Err("32-bit target is not supported (Zyringe is x86_64 only)".into()),
        Ok(_) => {}
        Err(e) => eprintln!("warning: could not read target ELF class: {e}"),
    }

    let dll_bytes =
        std::fs::read(&args.dll).map_err(|e| format!("cannot read DLL {}: {e}", args.dll))?;
    if dll_bytes.len() < 2 || &dll_bytes[0..2] != b"MZ" {
        return Err(format!("{} is not a PE/.NET assembly (no MZ header)", args.dll));
    }
    println!("loaded {} bytes of assembly", dll_bytes.len());

    let mono_so = maps::find_mono_so(args.pid).unwrap_or_default();
    if mono_so.is_empty() {
        eprintln!("warning: no Mono runtime found in target maps; helper will probe globally");
    } else {
        println!("target Mono runtime: {mono_so}");
    }


    let (target_helper, cleanup) = stage_helper(args.pid, &args.so);
    println!("helper visible to target at: {target_helper}");

    let res = ptrace_engine::inject(
        args.pid,
        &target_helper,
        &dll_bytes,
        &args.namespace,
        &args.class,
        &args.method,
        &mono_so,
        &args.dll,
        &args.args,
        args.timeout_secs,
        args.cancel_flag.as_deref(),
    );


    if let Some(path) = cleanup {
        let _ = std::fs::remove_file(path);
    }
    res
}


fn stage_helper(pid: i32, host_so: &str) -> (String, Option<String>) {
    let fallback = (host_so.to_string(), None);
    let bytes = match std::fs::read(host_so) {
        Ok(b) => b,
        Err(_) => return fallback,
    };


    let mut dirs: Vec<String> = Vec::new();
    for d in ["/tmp", "/dev/shm"] {
        if !mount_noexec(pid, d) {
            dirs.push(d.to_string());
        }
    }
    if let Some(exedir) = target_exe_dir(pid) {
        if !mount_noexec(pid, &exedir) {
            dirs.push(exedir);
        }
    }
    dirs.push("/tmp".to_string());

    for dir in dirs {
        let in_target = format!("{dir}/.zyringe-{pid}.so");
        let host_dest = format!("/proc/{pid}/root{in_target}");
        if std::fs::write(&host_dest, &bytes).is_ok() {
            let _ = std::fs::set_permissions(&host_dest, std::fs::Permissions::from_mode(0o755));
            if mount_noexec(pid, &dir) {
                eprintln!("warning: staged into {dir}, which looks noexec — dlopen may fail");
            }
            return (in_target, Some(host_dest));
        }
    }
    fallback
}


fn target_exe_dir(pid: i32) -> Option<String> {
    let exe = std::fs::read_link(format!("/proc/{pid}/exe")).ok()?;
    exe.parent()?.to_str().map(String::from)
}


fn path_under(path: &str, mount: &str) -> bool {
    mount == "/" || path == mount || path.starts_with(&format!("{mount}/"))
}


fn mount_noexec(pid: i32, path: &str) -> bool {
    let content = match std::fs::read_to_string(format!("/proc/{pid}/mountinfo")) {
        Ok(c) => c,
        Err(_) => return false,
    };
    let mut best_len = 0usize;
    let mut noexec = false;
    for line in content.lines() {
        let f: Vec<&str> = line.split(' ').collect();
        if f.len() < 6 {
            continue;
        }
        let (mount_point, options) = (f[4], f[5]);
        if path_under(path, mount_point) && mount_point.len() >= best_len {
            best_len = mount_point.len();
            noexec = options.split(',').any(|o| o == "noexec");
        }
    }
    noexec
}

fn main() {
    match run() {
        Ok(()) => {
            println!("done");
            exit(0);
        }
        Err(e) => {
            eprintln!("error: {e}");
            exit(1);
        }
    }
}
