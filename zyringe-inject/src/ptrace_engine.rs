

use crate::maps;
use nix::sys::ptrace;
use nix::sys::signal::Signal;
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::Pid;
use std::fs;
use std::os::raw::c_void;

const RTLD_NOW: u64 = 2;
const PROT_RW: u64 = 0x3;
const MAP_PRIV_ANON: u64 = 0x22;
const MAP_FAILED: u64 = u64::MAX;

const ZY_MAGIC: u32 = 0x5A595251;
const HDR_LEN: usize = 128;
const ERR_CAP: usize = 512;
const RET_CAP: usize = 512;

macro_rules! step {
    ($($a:tt)*) => {{ println!($($a)*); }};
}


fn write_mem(pid: i32, addr: u64, data: &[u8]) -> Result<(), String> {
    let local = libc::iovec {
        iov_base: data.as_ptr() as *mut c_void,
        iov_len: data.len(),
    };
    let remote = libc::iovec {
        iov_base: addr as *mut c_void,
        iov_len: data.len(),
    };
    let n = unsafe { libc::process_vm_writev(pid, &local, 1, &remote, 1, 0) };
    if n < 0 {
        return Err(format!("process_vm_writev: {}", std::io::Error::last_os_error()));
    }
    if n as usize != data.len() {
        return Err(format!("short write {}/{}", n, data.len()));
    }
    Ok(())
}

fn read_mem(pid: i32, addr: u64, len: usize) -> Result<Vec<u8>, String> {
    let mut buf = vec![0u8; len];
    let local = libc::iovec {
        iov_base: buf.as_mut_ptr() as *mut c_void,
        iov_len: len,
    };
    let remote = libc::iovec {
        iov_base: addr as *mut c_void,
        iov_len: len,
    };
    let n = unsafe { libc::process_vm_readv(pid, &local, 1, &remote, 1, 0) };
    if n < 0 {
        return Err(format!("process_vm_readv: {}", std::io::Error::last_os_error()));
    }
    buf.truncate(n as usize);
    Ok(buf)
}


struct LibcSyms {
    mmap: u64,
    dlopen: u64,
    dlsym: u64,
}


fn resolve_libc(pid: i32) -> Result<LibcSyms, String> {
    if let Some((path, base)) = maps::libc_mapping(pid) {
        let host_view = format!("/proc/{pid}/root{path}");
        if let Ok(data) = fs::read(&host_view) {
            if let Ok(elf) = crate::elf::Elf::parse(&data) {
                let bias = base.wrapping_sub(elf.min_vaddr);
                let mmap = elf.resolve("mmap");
                let dlopen = elf
                    .resolve("dlopen")
                    .or_else(|| elf.resolve("__libc_dlopen_mode"));
                let dlsym = elf.resolve("dlsym");
                if let (Some(m), Some(o), Some(s)) = (mmap, dlopen, dlsym) {
                    return Ok(LibcSyms {
                        mmap: bias + m,
                        dlopen: bias + o,
                        dlsym: bias + s,
                    });
                }
            }
        }
    }

    Ok(LibcSyms {
        mmap: local_translate(pid, "mmap")?,
        dlopen: local_translate(pid, "dlopen")?,
        dlsym: local_translate(pid, "dlsym")?,
    })
}

fn local_translate(pid: i32, name: &str) -> Result<u64, String> {
    let local_base = maps::load_base("/proc/self/maps", maps::is_libc)
        .ok_or("libc base not found in self maps")?;
    let remote_base = maps::load_base(&format!("/proc/{pid}/maps"), maps::is_libc)
        .ok_or("libc base not found in target maps")?;
    let cname = std::ffi::CString::new(name).unwrap();
    let local = unsafe { libc::dlsym(std::ptr::null_mut(), cname.as_ptr()) } as u64;
    if local == 0 {
        return Err(format!("dlsym({name}) returned NULL"));
    }
    if local < local_base {
        return Err(format!("{name} below libc base"));
    }
    Ok(remote_base + (local - local_base))
}


fn thread_ids(pid: i32) -> Vec<i32> {
    let mut v = Vec::new();
    if let Ok(rd) = fs::read_dir(format!("/proc/{pid}/task")) {
        for e in rd.flatten() {
            if let Some(t) = e.file_name().to_str().and_then(|s| s.parse::<i32>().ok()) {
                v.push(t);
            }
        }
    }
    if v.is_empty() {
        v.push(pid);
    }
    v
}

fn stop_all(pid: i32) -> Result<Vec<i32>, String> {
    let mut attached = Vec::new();
    for tid in thread_ids(pid) {
        let p = Pid::from_raw(tid);
        if ptrace::attach(p).is_err() {
            continue;
        }
        match waitpid(p, Some(WaitPidFlag::__WALL)) {
            Ok(_) => attached.push(tid),
            Err(e) => return Err(format!("waitpid(attach {tid}): {e}")),
        }
    }
    if attached.is_empty() {
        return Err("could not attach to any thread".into());
    }
    Ok(attached)
}

fn detach_all(tids: &[i32]) {
    for &t in tids {
        let _ = ptrace::detach(Pid::from_raw(t), None);
    }
}


fn pick_hijack(pid: i32, tids: &[i32]) -> Result<(i32, libc::user_regs_struct), String> {
    let mut best: Option<(i32, i32, libc::user_regs_struct)> = None;
    for &tid in tids {
        let regs = match ptrace::getregs(Pid::from_raw(tid)) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let parked = regs.rip >= 2
            && read_mem(pid, regs.rip - 2, 2)
                .map(|b| b == [0x0f, 0x05])
                .unwrap_or(false);
        let mut score = 0;
        if parked {
            score += 5;
        }
        if thread_state(pid, tid) == Some(b'S') {
            score += 2;
        }
        if wchan_benign(pid, tid) {
            score += 2;
        }
        if tid != pid {
            score += 1;
        }
        if best.as_ref().map_or(true, |(s, _, _)| score > *s) {
            best = Some((score, tid, regs));
        }
    }


    if let Some((score, tid, regs)) = best {
        if score >= 5 {
            return Ok((tid, regs));
        }
    }
    eprintln!("warning: no syscall-parked thread found; hijacking main thread (deadlock risk)");
    let regs = ptrace::getregs(Pid::from_raw(pid)).map_err(|e| format!("getregs(main): {e}"))?;
    Ok((pid, regs))
}


fn thread_state(pid: i32, tid: i32) -> Option<u8> {
    let s = fs::read_to_string(format!("/proc/{pid}/task/{tid}/stat")).ok()?;
    let close = s.rfind(')')?;
    s.as_bytes().get(close + 2).copied()
}


fn wchan_benign(pid: i32, tid: i32) -> bool {
    let w = match fs::read_to_string(format!("/proc/{pid}/task/{tid}/wchan")) {
        Ok(w) => w,
        Err(_) => return false,
    };
    const BENIGN: &[&str] = &[
        "futex", "poll", "select", "epoll", "read", "nanosleep", "wait",
        "sigtimedwait", "recv", "accept",
    ];
    BENIGN.iter().any(|k| w.contains(k))
}


const CALL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(12);

fn remote_call(
    pid: i32,
    tid: i32,
    pristine: &libc::user_regs_struct,
    func: u64,
    args: &[u64],
    cancel: &Cancel,
) -> Result<u64, String> {
    let t = Pid::from_raw(tid);
    let mut regs = *pristine;
    for (i, &a) in args.iter().enumerate() {
        match i {
            0 => regs.rdi = a,
            1 => regs.rsi = a,
            2 => regs.rdx = a,
            3 => regs.rcx = a,
            4 => regs.r8 = a,
            5 => regs.r9 = a,
            _ => return Err("too many args for remote_call".into()),
        }
    }

    let mut sp = pristine.rsp.wrapping_sub(512);
    sp &= !0xFu64;
    sp = sp.wrapping_sub(8);
    write_mem(pid, sp, &0u64.to_ne_bytes())?;
    regs.rsp = sp;
    regs.rip = func;
    regs.rax = 0;
    ptrace::setregs(t, regs).map_err(|e| format!("setregs: {e}"))?;

    let ret = run_until_trap(t, cancel);

    let _ = ptrace::setregs(t, *pristine);
    ret
}


fn run_until_trap(t: Pid, cancel: &Cancel) -> Result<u64, String> {
    let mut deliver: Option<Signal> = None;
    let deadline = std::time::Instant::now() + CALL_TIMEOUT;
    for _ in 0..1024 {
        ptrace::cont(t, deliver.take()).map_err(|e| format!("cont: {e}"))?;

        let status = loop {
            match waitpid(t, Some(WaitPidFlag::__WALL | WaitPidFlag::WNOHANG)) {
                Ok(WaitStatus::StillAlive) => {
                    if cancel.is_set() {
                        return Err("cancelled".into());
                    }
                    if std::time::Instant::now() > deadline {
                        return Err("remote call timed out — target may not be a live \
                                    Mono game (no initialized runtime?)"
                            .into());
                    }
                    std::thread::sleep(std::time::Duration::from_millis(2));
                }
                Ok(other) => break other,
                Err(e) => return Err(format!("waitpid: {e}")),
            }
        };
        match status {
            WaitStatus::Stopped(_, sig) => {
                let regs = ptrace::getregs(t).map_err(|e| format!("getregs: {e}"))?;
                if (sig == Signal::SIGSEGV || sig == Signal::SIGTRAP) && regs.rip == 0 {
                    return Ok(regs.rax);
                }
                if sig == Signal::SIGSEGV {
                    return Err(format!("unexpected SIGSEGV at rip=0x{:x}", regs.rip));
                }
                deliver = Some(sig);
            }
            WaitStatus::Exited(_, c) => return Err(format!("target exited (code {c})")),
            WaitStatus::Signaled(_, s, _) => return Err(format!("target killed by {s:?}")),
            other => return Err(format!("unexpected wait: {other:?}")),
        }
    }
    Err("remote call did not return".into())
}


pub struct Cancel {
    path: Option<std::path::PathBuf>,
}
impl Cancel {
    fn new(path: Option<&str>) -> Self {
        Cancel {
            path: path.map(std::path::PathBuf::from),
        }
    }
    fn is_set(&self) -> bool {
        self.path.as_ref().is_some_and(|p| p.exists())
    }
    fn clear(&self) {
        if let Some(p) = &self.path {
            let _ = std::fs::remove_file(p);
        }
    }
}


struct Layout {
    total: usize,
    ns: usize,
    class: usize,
    method: usize,
    mono_so: usize,
    path: usize,
    helper: usize,
    zrun: usize,
    error: usize,
    ret: usize,
    args: usize,
    dll: usize,
}

fn align(x: usize, a: usize) -> usize {
    (x + a - 1) & !(a - 1)
}

#[allow(clippy::too_many_arguments)]
fn plan(
    dll_len: usize,
    ns: &[u8],
    class: &[u8],
    method: &[u8],
    mono_so: &[u8],
    path: &[u8],
    helper: &[u8],
    zrun: &[u8],
    args_total: usize,
) -> Layout {
    let mut c = HDR_LEN;
    let ns_o = c;
    c += ns.len() + 1;
    let class_o = c;
    c += class.len() + 1;
    let method_o = c;
    c += method.len() + 1;
    let mono_o = c;
    c += mono_so.len() + 1;
    let path_o = c;
    c += path.len() + 1;
    let helper_o = c;
    c += helper.len() + 1;
    let zrun_o = c;
    c += zrun.len() + 1;
    c = align(c, 8);
    let error_o = c;
    c += ERR_CAP;
    let ret_o = c;
    c += RET_CAP;
    let args_o = c;
    c += args_total.max(1);
    c = align(c, 16);
    let dll_o = c;
    c += dll_len;
    Layout {
        total: align(c, 4096),
        ns: ns_o,
        class: class_o,
        method: method_o,
        mono_so: mono_o,
        path: path_o,
        helper: helper_o,
        zrun: zrun_o,
        error: error_o,
        ret: ret_o,
        args: args_o,
        dll: dll_o,
    }
}


#[allow(clippy::too_many_arguments)]
pub fn inject(
    pid: i32,
    helper_so: &str,
    dll_bytes: &[u8],
    namespace: &str,
    class: &str,
    method: &str,
    mono_so: &str,
    dll_path: &str,
    args: &[String],
    timeout_secs: Option<u32>,
    cancel_flag: Option<&str>,
) -> Result<(), String> {

    let ns_c = cstr(namespace);
    let class_c = cstr(class);
    let method_c = cstr(method);
    let mono_c = cstr(mono_so);
    let path_c = cstr(dll_path);
    let helper_c = cstr(helper_so);
    let zrun_c = cstr("zyringe_run");


    let mut args_block: Vec<u8> = Vec::new();
    for a in args {
        args_block.extend_from_slice(a.as_bytes());
        args_block.push(0);
    }
    let argc = args.len() as u32;


    let cancel = Cancel::new(cancel_flag);
    cancel.clear();

    step!("resolving libc symbols in target…");
    let libc = resolve_libc(pid)?;
    let (mmap_addr, dlopen_addr, dlsym_addr) = (libc.mmap, libc.dlopen, libc.dlsym);
    step!("mmap@0x{mmap_addr:x} dlopen@0x{dlopen_addr:x} dlsym@0x{dlsym_addr:x}");

    let lay = plan(
        dll_bytes.len(),
        &ns_c,
        &class_c,
        &method_c,
        &mono_c,
        &path_c,
        &helper_c,
        &zrun_c,
        args_block.len(),
    );

    step!("stopping target threads…");
    let attached = stop_all(pid)?;
    let (tid, pristine) = pick_hijack(pid, &attached)?;
    step!(
        "stopped {} thread(s); hijacking tid {} {}",
        attached.len(),
        tid,
        if tid == pid { "(main)" } else { "(syscall-parked)" }
    );


    let staged = (|| -> Result<u64, String> {

        let base = remote_call(
            pid,
            tid,
            &pristine,
            mmap_addr,
            &[0, lay.total as u64, PROT_RW, MAP_PRIV_ANON, u64::MAX, 0],
            &cancel,
        )?;
        if base == MAP_FAILED || base == 0 {
            return Err("remote mmap failed".into());
        }
        step!("allocated {} bytes @ 0x{:x}", lay.total, base);


        let buf = build_buffer(base, &lay, dll_bytes, &ns_c, &class_c, &method_c, &mono_c,
            &path_c, &helper_c, &zrun_c, &args_block, argc);
        write_mem(pid, base, &buf)?;


        step!("loading helper into target…");
        let handle = remote_call(pid, tid, &pristine, dlopen_addr,
            &[base + lay.helper as u64, RTLD_NOW], &cancel)?;
        if handle == 0 {
            return Err("dlopen(helper) returned NULL in target".into());
        }
        let zrun = remote_call(pid, tid, &pristine, dlsym_addr,
            &[handle, base + lay.zrun as u64], &cancel)?;
        if zrun == 0 {
            return Err("dlsym(zyringe_run) returned NULL".into());
        }
        let rc = remote_call(pid, tid, &pristine, zrun, &[base], &cancel)?;
        if (rc as u32) != 0 {
            return Err(format!("zyringe_run rejected the request (rc={})", rc as i32));
        }
        step!("helper armed; worker thread waiting for go");
        Ok(base)
    })();


    let _ = ptrace::setregs(Pid::from_raw(tid), pristine);
    detach_all(&attached);
    step!("detached; target resumed");

    let base = staged?;


    write_mem(pid, base + 4, &1u32.to_ne_bytes())?;


    step!("waiting for managed entry point…");
    let out = poll_result(pid, base, &lay, &cancel, timeout_secs.unwrap_or(40));
    cancel.clear();
    out
}

fn poll_result(pid: i32, base: u64, lay: &Layout, cancel: &Cancel, timeout_secs: u32) -> Result<(), String> {


    let deadline =
        std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs.max(5) as u64);
    loop {
        let s = read_mem(pid, base + 8, 4)?;
        let status = u32::from_ne_bytes([s[0], s[1], s[2], s[3]]);
        match status {
            1 => {

                let r = read_mem(pid, base + lay.ret as u64, RET_CAP).unwrap_or_default();
                let ret = cstr_to_string(&r);
                if ret.is_empty() {
                    step!("managed entry point executed");
                } else {
                    step!("managed entry point executed → returned: {ret}");
                }
                return Ok(());
            }
            2 => {
                let e = read_mem(pid, base + lay.error as u64, ERR_CAP).unwrap_or_default();
                let msg = cstr_to_string(&e);
                return Err(if msg.is_empty() {
                    "helper reported failure".into()
                } else {
                    msg
                });
            }
            _ => {}
        }
        if cancel.is_set() {
            return Err("cancelled by user".into());
        }
        if std::time::Instant::now() > deadline {
            return Err("timed out waiting for the managed entry point — the target may \
                        not be the real game process (duplicate PID?)"
                .into());
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}


fn cstr(s: &str) -> Vec<u8> {
    let mut v = s.as_bytes().to_vec();
    v.push(0);
    v
}

fn cstr_to_string(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

#[allow(clippy::too_many_arguments)]
fn build_buffer(
    base: u64,
    lay: &Layout,
    dll: &[u8],
    ns: &[u8],
    class: &[u8],
    method: &[u8],
    mono_so: &[u8],
    path: &[u8],
    helper: &[u8],
    zrun: &[u8],
    args: &[u8],
    argc: u32,
) -> Vec<u8> {
    let mut buf = vec![0u8; lay.total];
    let put = |buf: &mut [u8], off: usize, data: &[u8]| {
        buf[off..off + data.len()].copy_from_slice(data);
    };
    put(&mut buf, lay.ns, ns);
    put(&mut buf, lay.class, class);
    put(&mut buf, lay.method, method);
    put(&mut buf, lay.mono_so, mono_so);
    put(&mut buf, lay.path, path);
    put(&mut buf, lay.helper, helper);
    put(&mut buf, lay.zrun, zrun);
    put(&mut buf, lay.args, args);
    put(&mut buf, lay.dll, dll);


    let mut h = [0u8; HDR_LEN];
    h[0..4].copy_from_slice(&ZY_MAGIC.to_ne_bytes());

    let u32at = |h: &mut [u8], off: usize, v: u32| h[off..off + 4].copy_from_slice(&v.to_ne_bytes());
    let u64at = |h: &mut [u8], off: usize, v: u64| h[off..off + 8].copy_from_slice(&v.to_ne_bytes());
    u64at(&mut h, 16, base + lay.dll as u64);
    u64at(&mut h, 24, dll.len() as u64);
    u64at(&mut h, 32, base + lay.ns as u64);
    u64at(&mut h, 40, base + lay.class as u64);
    u64at(&mut h, 48, base + lay.method as u64);
    u64at(&mut h, 56, base + lay.mono_so as u64);
    u64at(&mut h, 64, base + lay.error as u64);
    u64at(&mut h, 72, ERR_CAP as u64);
    u64at(&mut h, 80, base + lay.path as u64);
    u32at(&mut h, 88, argc);

    u64at(&mut h, 96, base + lay.args as u64);
    u64at(&mut h, 104, base + lay.ret as u64);
    u64at(&mut h, 112, RET_CAP as u64);
    u64at(&mut h, 120, lay.total as u64);
    buf[0..HDR_LEN].copy_from_slice(&h);
    buf
}
