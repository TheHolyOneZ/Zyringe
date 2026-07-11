

use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};


const MONO_MARKERS: &[(&str, &str)] = &[
    ("libmonobdwgc-2.0", "Mono BleedingEdge"),
    ("libmonosgen-2.0", "Mono SGen"),
    ("libmono-2.0", "Mono 2.0"),
    ("libmonoboehm", "Mono Boehm"),
    ("libmono", "Mono"),
];

#[derive(Debug, Clone, Serialize)]
pub struct MonoProcess {
    pub pid: i32,

    pub name: String,

    pub cmdline: String,

    pub flavor: String,

    pub mono_so_path: String,

    pub data_dir: Option<String>,


    pub game_dir: Option<String>,

    pub exe_path: Option<String>,


    pub thread_count: usize,


    pub suspect: bool,

    pub engine: String,


    pub injectable: bool,


    pub duplicate: bool,

    pub app_id: Option<String>,


    pub proton: bool,
}


pub fn scan() -> Vec<MonoProcess> {
    let mut out = Vec::new();
    let self_pid = std::process::id() as i32;

    let entries = match fs::read_dir("/proc") {
        Ok(e) => e,
        Err(_) => return out,
    };

    for entry in entries.flatten() {
        let pid = match entry.file_name().to_str().and_then(|s| s.parse::<i32>().ok()) {
            Some(p) => p,
            None => continue,
        };
        if pid == self_pid {
            continue;
        }
        if let Some(proc) = inspect_pid(pid) {
            out.push(proc);
        }
    }

    let mut out = mark_duplicates(out);

    out.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then(a.duplicate.cmp(&b.duplicate))
            .then(a.pid.cmp(&b.pid))
    });
    out
}


fn inspect_pid(pid: i32) -> Option<MonoProcess> {
    let maps_path = format!("/proc/{pid}/maps");


    let maps = fs::read_to_string(&maps_path).ok()?;


    let (engine, flavor, mono_so_path, injectable) = match find_mono_mapping(&maps) {

        Some((so, fl)) => ("Mono".to_string(), fl, so, true),
        None => {
            if let Some(fl) = find_windows_mono(&maps) {


                ("Mono".to_string(), fl, String::new(), false)
            } else if maps_has_il2cpp(&maps) {
                ("IL2CPP".to_string(), "IL2CPP".to_string(), String::new(), false)
            } else {
                return None;
            }
        }
    };

    let name = fs::read_to_string(format!("/proc/{pid}/comm"))
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| format!("pid {pid}"));

    let cmdline = read_cmdline(pid).unwrap_or_default();
    let exe_path = fs::read_link(format!("/proc/{pid}/exe"))
        .ok()
        .and_then(|p| p.to_str().map(String::from));
    let data_dir = exe_path.as_deref().and_then(derive_data_dir);

    let thread_count = fs::read_dir(format!("/proc/{pid}/task"))
        .map(|rd| rd.flatten().count())
        .unwrap_or(1);
    let suspect = mono_so_path.contains("native");
    let app_id = read_env(pid, "SteamAppId").or_else(|| read_env(pid, "SteamGameId"));
    let proton = read_env(pid, "STEAM_COMPAT_DATA_PATH").is_some()
        || read_env(pid, "WINEPREFIX").is_some()
        || read_env(pid, "PROTON_VERSION").is_some();


    let game_dir = read_env(pid, "STEAM_COMPAT_INSTALL_PATH")
        .filter(|p| !p.is_empty())
        .or_else(|| parent_of(data_dir.as_deref()))
        .or_else(|| {
            exe_path
                .as_deref()
                .filter(|&e| !looks_like_wine(e))
                .and_then(|e| parent_of(Some(e)))
        })
        .or_else(|| wine_game_dir(pid))
        .or_else(|| proc_cwd(pid));

    Some(MonoProcess {
        pid,
        name,
        cmdline,
        flavor,
        mono_so_path,
        data_dir,
        game_dir,
        exe_path,
        thread_count,
        suspect,
        engine,
        injectable,
        duplicate: false,
        app_id,
        proton,
    })
}


fn read_env(pid: i32, key: &str) -> Option<String> {
    let raw = fs::read(format!("/proc/{pid}/environ")).ok()?;
    let prefix = format!("{key}=");
    for part in raw.split(|&b| b == 0) {
        if let Ok(s) = std::str::from_utf8(part) {
            if let Some(v) = s.strip_prefix(&prefix) {
                return Some(v.to_string());
            }
        }
    }
    None
}


fn maps_has_il2cpp(maps: &str) -> bool {
    for line in maps.lines() {
        let path = match line.splitn(6, ' ').nth(5) {
            Some(p) => p.trim(),
            None => continue,
        };
        let file = Path::new(path)
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or("");
        if file.contains("GameAssembly") || file.contains("libil2cpp") {
            return true;
        }
    }
    false
}


fn mark_duplicates(mut procs: Vec<MonoProcess>) -> Vec<MonoProcess> {

    procs.sort_by(|a, b| b.thread_count.cmp(&a.thread_count));
    let mut seen = std::collections::HashSet::new();
    for p in procs.iter_mut() {
        if let Some(exe) = &p.exe_path {
            if !seen.insert(exe.clone()) {
                p.duplicate = true;
            }
        }
    }
    procs
}


fn find_mono_mapping(maps: &str) -> Option<(String, String)> {
    let mut native_fallback: Option<(String, String)> = None;
    for line in maps.lines() {

        let path = match line.splitn(6, ' ').nth(5) {
            Some(p) => p.trim(),
            None => continue,
        };
        if path.is_empty() {
            continue;
        }
        let file = Path::new(path)
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or("");
        for (marker, flavor) in MONO_MARKERS {
            if file.contains(marker) {
                if file.contains("native") {
                    native_fallback
                        .get_or_insert_with(|| (path.to_string(), (*flavor).to_string()));
                } else {
                    return Some((path.to_string(), (*flavor).to_string()));
                }
                break;
            }
        }
    }
    native_fallback
}


fn find_windows_mono(maps: &str) -> Option<String> {
    for line in maps.lines() {
        let path = match line.splitn(6, ' ').nth(5) {
            Some(p) => p.trim(),
            None => continue,
        };
        let file = Path::new(path)
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !file.ends_with(".dll") {
            continue;
        }
        if file.contains("monobdwgc") || file.contains("mono-2.0-bdwgc") {
            return Some("Mono BleedingEdge (Proton)".into());
        }
        if file.contains("monosgen") || file.contains("mono-2.0-sgen") {
            return Some("Mono SGen (Proton)".into());
        }
        if file == "mono.dll" || file.contains("mono-2.0") {
            return Some("Mono (Proton)".into());
        }
    }
    None
}


fn looks_like_wine(exe: &str) -> bool {
    let f = Path::new(exe)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    f.starts_with("wine")
        || f.contains("wine")
        || f.contains("proton")
        || f.contains("preloader")
        || f == "pv-bwrap"
        || f == "srt-bwrap"
}


fn proc_cwd(pid: i32) -> Option<String> {
    fs::read_link(format!("/proc/{pid}/cwd"))
        .ok()
        .and_then(|p| p.to_str().map(String::from))
}


fn wine_game_dir(pid: i32) -> Option<String> {

    let raw = fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    let win = raw
        .split(|&b| b == 0)
        .filter_map(|s| std::str::from_utf8(s).ok())
        .find(|t| {
            let l = t.to_ascii_lowercase();
            l.ends_with(".exe") && (t.contains('\\') || t.contains(':'))
        })?
        .replace('\\', "/");

    let (drive, rest) = win.split_once(":/").or_else(|| win.split_once(':'))?;
    let letter = drive.chars().last()?.to_ascii_lowercase();
    let rest = rest.trim_start_matches('/');
    let linux_exe = if letter == 'z' {
        format!("/{rest}")
    } else {
        let prefix = read_env(pid, "WINEPREFIX")
            .or_else(|| read_env(pid, "STEAM_COMPAT_DATA_PATH").map(|d| format!("{d}/pfx")))?;
        format!("{}/drive_{}/{}", prefix.trim_end_matches('/'), letter, rest)
    };
    let dir = Path::new(&linux_exe).parent()?;
    if dir.is_dir() {
        dir.to_str().map(String::from)
    } else {
        None
    }
}


fn read_cmdline(pid: i32) -> Option<String> {
    let raw = fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    if raw.is_empty() {
        return None;
    }
    let joined = raw
        .split(|&b| b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .collect::<Vec<_>>()
        .join(" ");
    if joined.is_empty() {
        None
    } else {
        Some(joined)
    }
}


fn parent_of(path: Option<&str>) -> Option<String> {
    let p = path?;
    Path::new(p).parent().and_then(|d| d.to_str()).map(String::from)
}


fn derive_data_dir(exe_path: &str) -> Option<String> {
    let exe = Path::new(exe_path);
    let dir = exe.parent()?;
    let stem = exe.file_stem()?.to_str()?;
    let candidate: PathBuf = dir.join(format!("{stem}_Data"));
    if candidate.is_dir() {
        return candidate.to_str().map(String::from);
    }

    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                if let Some(n) = p.file_name().and_then(|n| n.to_str()) {
                    if n.ends_with("_Data") {
                        return p.to_str().map(String::from);
                    }
                }
            }
        }
    }
    None
}
